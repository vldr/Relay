use std::{collections::HashMap, rc::{Rc}, cell::{RefCell}};
use ws::{Handler, Message, Result, Sender};
use serde::{Deserialize, Serialize};
use uuid::{Uuid};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReceivePacket {
    CreateRoom { size: Option<u8> },
    JoinRoom { id: String },
    LeaveRoom,
} 

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransmitPacket {
    CreateRoom { id: String },
    LeaveRoom { index: usize },
    JoinRoom,

    Kick { message: String },
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
    size: u8,
    senders: Vec<Sender>,
}

impl Room 
{
    fn new(size: u8) -> Room
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
    hosts: HashMap<Sender, SenderTuple>,
}

impl Server 
{
    pub fn new() -> Rc<RefCell<Server>>
    {
        Rc::new(
            RefCell::new(
                Server{
                    rooms: HashMap::new(),
                    hosts: HashMap::new(),
                }
            )
        )
    }
}

#[derive(Clone)]
pub struct SenderTuple {
    index: usize,
    host: Sender
}

#[derive(Clone)]
pub struct Client 
{
    server: Rc<RefCell<Server>>,

    sender: Sender,
    room_id: Option<String>
}

impl Client
{
    const DEFAULT_ROOM_SIZE: u8 = 2;

    pub fn new(server: Rc<RefCell<Server>>, sender: Sender) -> Client
    {
        Client { server, sender, room_id: None }
    }

    fn handle_create_room(&mut self, size_option: Option<u8>) -> Result<()>
    {
        let mut server = self.server.borrow_mut();

        let size = size_option.unwrap_or(Self::DEFAULT_ROOM_SIZE);
        if size == 0 
        {
            return self.sender.send_error_packet(format!("You cannot create an empty room."));
        }

        if server.rooms.iter().any(|(_, room)| room.senders.iter().any(|sender| *sender == self.sender)) 
        {
            return self.sender.send_error_packet(format!("You're currently in a room."));
        }

        let room_id = Uuid::new_v4().to_string();
        self.room_id = Some(room_id.clone());

        let mut room = Room::new(size);
        room.senders.push(self.sender.clone());
        
        server.rooms.insert(room_id.clone(), room);
        
        return self.sender.send_packet(TransmitPacket::CreateRoom { id: room_id.clone() });
    }

    fn handle_join_room(&self, id: String) -> Result<()>
    {
        let mut server = self.server.borrow_mut();

        if server.rooms.iter().any(|(_, room)| room.senders.iter().any(|sender| *sender == self.sender)) 
        {
            return self.sender.send_error_packet(format!("You're already in a room."));
        }

        if let Some(room) = server.rooms.get_mut(&id)
        {
            if room.senders.len() >= room.size.into()
            {
                return self.sender.send_error_packet(format!("The room is full."));
            }
            
            room.senders.push(self.sender.clone());
            room.senders[0].send_packet(TransmitPacket::JoinRoom)?;

            let sender_tuple = SenderTuple { 
                host: room.senders[0].clone(), 
                index: room.senders.len() - 1,
            };

            server.hosts.insert(self.sender.clone(), sender_tuple);

            return self.sender.send_packet(TransmitPacket::JoinRoom);
        }
        
        return self.sender.send_error_packet(format!("The room '{}' does not exist.", id));  
    }

    fn handle_leave_room(&mut self) -> Result<()> 
    {
        let mut server = self.server.borrow_mut();

        if let Some(sender_tuple) = server.hosts.remove(&self.sender) 
        {
            for (_, room) in &mut server.rooms 
            {
                for (index, sender) in room.senders.iter().enumerate()
                {
                    if *sender == self.sender 
                    {
                        room.senders.remove(index);

                        return sender_tuple.host.send_packet(TransmitPacket::LeaveRoom { index });
                    }
                }
            }
        }
        else if let Some(room_id) = self.room_id.clone()
        {
            if let Some(room) = server.rooms.remove(&room_id)
            {
                for sender in room.senders 
                {
                    if sender == self.sender 
                    {
                        continue;
                    }

                    server.hosts.remove(&sender);

                    sender.send_packet(TransmitPacket::Kick { message: format!("The host has left the room.") })?;
                }
            }

            self.room_id = None;
        }

        return Ok(());
    } 
}

impl Handler for Client
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
                        ReceivePacket::CreateRoom { size } => self.handle_create_room(size)?,
                        ReceivePacket::JoinRoom { id } => self.handle_join_room(id)?,
                        ReceivePacket::LeaveRoom => self.handle_leave_room()?,
                    }
                }
                else 
                {
                    return self.sender.send_error_packet(format!("The following packet is invalid: {}", text));
                }
            }
        }
        else if message.is_binary()
        {
            let server = self.server.borrow();

            if let Some(sender_tuple) = server.hosts.get(&self.sender) 
            {
                let mut data = message.into_data();
                data.insert(0, sender_tuple.index.try_into().unwrap());

                return sender_tuple.host.send(data);  
            }
            else if let Some(room_id) = self.room_id.clone()
            {
                if let Some(room) = server.rooms.get(&room_id) 
                {
                    let data = message.into_data();

                    if data.len() > 0
                    {
                        let index = data[0];
                        let message = Message::Binary(data[1..].to_vec());

                        if index > 0 && usize::from(index) < room.senders.len()
                        {
                            return room.senders[usize::from(index)].send(message);
                        }
                        else if index == 0
                        {
                            for sender in &room.senders 
                            {   
                                if *sender == self.sender 
                                {
                                    continue;
                                }
        
                                sender.send(message.clone())?;                            
                            }

                            return Ok(());
                        }
                    }
                }
            }

            return self.sender.send_error_packet(format!("You're not currently in a room."));
        }

        return Ok(());
    } 
}