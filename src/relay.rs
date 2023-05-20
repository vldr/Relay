use std::{collections::HashMap, cell::{RefCell}};
use ws::{Handler, Message, Result, Sender};
use serde::{Deserialize, Serialize};
use uuid::{Uuid};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ReceivePacket {
    Join { id: String },
    Create { size: Option<usize> },
    Leave,
} 

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransmitPacket {
    Create { id: String },
    Join { size: usize },
    Leave { index: usize },
    Error { message: String },
}

trait PacketSender 
{
    fn send_packet(&self, packet: TransmitPacket) -> Result<()>;
    fn send_error_packet(&self, message: String) -> Result<()>;
}

impl PacketSender for Sender 
{
    fn send_packet(&self, packet: TransmitPacket) -> Result<()>
    {
        let serialized_packet = serde_json::to_string(&packet).unwrap();

        self.send(Message::Text(serialized_packet))
    }

    fn send_error_packet(&self, message: String) -> Result<()>
    {
        let error_packet = TransmitPacket::Error { message };

        self.send_packet(error_packet)
    }
}

struct Room 
{
    size: usize,
    senders: Vec<Sender>,
}

impl Room 
{
    const MIN_ROOM_SIZE: usize = 0;
    const MAX_ROOM_SIZE: usize = 255;
    const DEFAULT_ROOM_SIZE: usize = 2;

    fn new(size: usize) -> Room
    {
        Room {
            senders: Vec::new(),
            size,
        }
    }
}

pub struct Server 
{
    rooms: HashMap<String, Room>,
}

impl Server 
{
    pub fn new() -> RefCell<Server>
    {
        RefCell::new(
            Server{
                rooms: HashMap::new(),
            }
        )
    }
}

pub struct Client<'server>
{
    server: &'server RefCell<Server>,
    sender: Sender,

    room_id: Option<String>
}

impl<'server> Client<'server>
{
    pub fn new(server: &'server RefCell<Server>, sender: Sender) -> Client<'server>
    {
        Client { server, sender, room_id: None }
    }

    fn handle_create_room(&mut self, size_option: Option<usize>) -> Result<()>
    {
        let mut server = self.server.borrow_mut();

        let size = size_option.unwrap_or(Room::DEFAULT_ROOM_SIZE);
        if size == Room::MIN_ROOM_SIZE || size >= Room::MAX_ROOM_SIZE
        {
            return self.sender.send_error_packet(format!("The room size '{}' is not valid", size));
        }

        if server.rooms.iter().any(|(_, room)| room.senders.iter().any(|sender| *sender == self.sender)) 
        {
            return self.sender.send_error_packet("You're currently in a room.".to_string());
        }

        let room_id = Uuid::new_v4().to_string();
        self.room_id = Some(room_id.clone());

        let mut room = Room::new(size);
        room.senders.push(self.sender.clone());
        server.rooms.insert(room_id.clone(), room);
        
        self.sender.send_packet(TransmitPacket::Create { id: room_id })
    }

    fn handle_join_room(&mut self, room_id: String) -> Result<()>
    {
        let mut server = self.server.borrow_mut();

        if server.rooms.iter().any(|(_, room)| room.senders.iter().any(|sender| *sender == self.sender)) 
        {
            return self.sender.send_error_packet("You're already in a room.".to_string());
        }

        if let Some(room) = server.rooms.get_mut(&room_id)
        {
            if room.senders.len() >= room.size
            {
                return self.sender.send_error_packet("The room is full.".to_string());
            }

            room.senders.push(self.sender.clone());

            for sender in &room.senders 
            {
                sender.send_packet(TransmitPacket::Join { size: room.senders.len() - 1 })?;
            }

            self.room_id = Some(room_id);
        }
        else 
        {
            return self.sender.send_error_packet(format!("The room '{}' does not exist.", room_id)); 
        }
        
        Ok(())
    }

    fn handle_leave_room(&mut self) -> Result<()> 
    {
        let mut server = self.server.borrow_mut();

        if let Some(room_id) = &self.room_id
        {
            if let Some(room) = server.rooms.get_mut(room_id)
            {
                if let Some(index) = room.senders.iter().position(|sender| *sender == self.sender)
                {
                    room.senders.remove(index);

                    for sender in &room.senders
                    {
                        sender.send_packet(TransmitPacket::Leave { index })?;
                    }
                }

                if room.senders.is_empty()
                {
                    server.rooms.remove(room_id);
                }
            }

            self.room_id = None;
        }

        Ok(())
    } 
}

impl<'server> Handler for Client<'server>
{
    fn on_close(&mut self, _: ws::CloseCode, _: &str) 
    {
        if let Err(error) = self.handle_leave_room() 
        {
            println!("Failed to leave room: {}", error);    
        }
    }

    fn on_message(&mut self, message: Message) -> Result<()> 
    {
        if message.is_text() 
        {
            if let Ok(text) = message.into_text()
            {
                if let Ok(packet) = serde_json::from_str(&text) 
                {
                    match packet 
                    {
                        ReceivePacket::Create { size } => self.handle_create_room(size)?,
                        ReceivePacket::Join { id } => self.handle_join_room(id)?,
                        ReceivePacket::Leave => self.handle_leave_room()?,
                    }
                }
            }
        }
        else if message.is_binary()
        {
            let server = self.server.borrow();

            if let Some(room_id) = &self.room_id
            {
                if let Some(room) = server.rooms.get(room_id) 
                {
                    if let Some(index) = room.senders.iter().position(|sender| *sender == self.sender)
                    {
                        let mut data = message.into_data();

                        if !data.is_empty()
                        {
                            let source = u8::try_from(index).unwrap();
                            let destination = usize::from(data[0]);
                            
                            data[0] = source;

                            if destination < room.senders.len()
                            {
                                return room.senders[destination].send(data);
                            }
                            else if destination == usize::from(u8::MAX)
                            {
                                for sender in &room.senders 
                                {   
                                    if *sender == self.sender 
                                    {
                                        continue;
                                    }
            
                                    sender.send(data.clone())?;                            
                                }

                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    } 
}