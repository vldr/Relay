use std::{collections::HashMap, rc::{Rc}, cell::{UnsafeCell}};
use ws::{Handler, Message, Result, Sender};
use serde::{Deserialize, Serialize};
use uuid::{Uuid};

macro_rules! get_relay {
    ($self:expr) => {
        unsafe { &mut *$self.relay.get() }
    };
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ReceivePacket {
    CreateRoom { size: Option<u8> },
    JoinRoom { id: String },
    LeaveRoom,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TransmitPacket {
    JoinRoom,
    CreateRoom { id: String },
    LeaveRoom { index: usize },
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

pub struct Relay 
{
    rooms: HashMap<String, Room>,
    hosts: HashMap<Sender, SenderTuple>,
}

impl Relay 
{
    pub fn new() -> Rc<UnsafeCell<Relay>>
    {
        Rc::new(
            UnsafeCell::new(
                Relay{
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
    relay: Rc<UnsafeCell<Relay>>,

    sender: Sender,
    room_id: Option<String>
}

impl Client
{
    const DEFAULT_ROOM_SIZE: u8 = 2;

    pub fn new(relay: Rc<UnsafeCell<Relay>>, sender: Sender) -> Client
    {
        Client { relay, sender, room_id: None }
    }

    fn handle_create_room(&mut self, size_option: Option<u8>) -> Result<()>
    {
        let relay = get_relay!(self);

        let size = size_option.unwrap_or(Self::DEFAULT_ROOM_SIZE);
        if size == 0 
        {
            return self.sender.send_error_packet(format!("The size value of '{}' is not valid.", size));
        }

        if relay.rooms.iter().any(|(_, room)| room.senders.iter().any(|sender| *sender == self.sender)) 
        {
            return self.sender.send_error_packet(format!("You're already in a room."));
        }

        let room_id = Uuid::new_v4().to_string();
        self.room_id = Some(room_id.clone());

        let mut room = Room::new(size);
        room.senders.push(self.sender.clone());
        
        relay.rooms.insert(room_id.clone(), room);
        
        return self.sender.send_packet(TransmitPacket::CreateRoom { id: room_id.clone() });
    }

    fn handle_join_room(&self, id: String) -> Result<()>
    {
        let relay = get_relay!(self);

        if relay.rooms.iter().any(|(_, room)| room.senders.iter().any(|sender| *sender == self.sender)) 
        {
            return self.sender.send_error_packet(format!("You're already in a room."));
        }

        if let Some(room) = relay.rooms.get_mut(&id)
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

            relay.hosts.insert(self.sender.clone(), sender_tuple);

            return self.sender.send_packet(TransmitPacket::JoinRoom);
        }
        
        return self.sender.send_error_packet(format!("The room '{}' does not exist.", id));  
    }

    fn handle_leave_room(&mut self) -> Result<()> 
    {
        let relay = get_relay!(self);

        if let Some(sender_tuple) = relay.hosts.remove(&self.sender) 
        {
            for (_, room) in &mut relay.rooms 
            {
                let mut index = 0;

                for sender in &room.senders 
                {
                    if *sender == self.sender 
                    {
                        room.senders.remove(index);

                        return sender_tuple.host.send_packet(TransmitPacket::LeaveRoom { index });
                    }

                    index += 1;
                }
            }
        }
        else if let Some(room_id) = self.room_id.clone()
        {
            if let Some(room) = relay.rooms.remove(&room_id)
            {
                for sender in room.senders 
                {
                    if sender == self.sender 
                    {
                        continue;
                    }

                    relay.hosts.remove(&sender);

                    sender.send_error_packet(format!("The host has left the room."))?;
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
            }
        }
        else if message.is_binary()
        {
            let relay = get_relay!(self);

            if let Some(sender_tuple) = relay.hosts.get(&self.sender) 
            {
                let mut data = message.into_data();
                data.insert(0, sender_tuple.index.try_into().unwrap());

                return sender_tuple.host.send(data);  
            }
            else if let Some(room_id) = self.room_id.clone()
            {
                if let Some(room) = relay.rooms.get(&room_id) 
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