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
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TransmitPacket {
    JoinRoom,
    CreateRoom { id: String },
    Error { message: String },
}

struct Room 
{
    id: String,
    size: u8,
    clients: Vec<Client>,
}

impl Room 
{
    fn new(size: u8) -> Room
    {
        Room {
            id: Uuid::new_v4().to_string(),
            clients: Vec::new(),
            size,
        }
    }
}

pub struct Relay 
{
    rooms: HashMap<String, Room>,
    hosts: HashMap<Sender, Sender>,
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
pub struct Client 
{
    relay: Rc<UnsafeCell<Relay>>,
    sender: Sender,

    room: Option<String>
}

impl Client
{
    const DEFAULT_ROOM_SIZE: u8 = 2;

    pub fn new(relay: Rc<UnsafeCell<Relay>>, sender: Sender) -> Client
    {
        Client { relay, sender, room: None }
    }

    fn handle_create_room(&mut self, size_option: Option<u8>) -> Result<()>
    {
        let relay = get_relay!(self);

        let size = size_option.unwrap_or(Self::DEFAULT_ROOM_SIZE);
        if size == 0 
        {
            return self.send_error_packet(format!("The size value of '{}' is not valid.", size));
        }

        if relay.rooms.iter().any(|(_, room)| room.clients.iter().any(|client| client.sender == self.sender)) 
        {
            return self.send_error_packet(format!("You're already in a room."));
        }

        let mut room = Room::new(size);
        room.clients.push(self.clone());

        self.room = Some(room.id.clone());

        let packet = TransmitPacket::CreateRoom { id: room.id.clone() };
        relay.rooms.insert(room.id.clone(), room);

        return self.send_packet(packet);
    }

    fn handle_join_room(&self, id: String) -> Result<()>
    {
        let relay = get_relay!(self);

        if let Some(room) = relay.rooms.get_mut(&id)
        {
            if relay.hosts.iter().any(|(sender, _)| *sender == self.sender) 
            {
                return self.send_error_packet(format!("You're already in a room."));
            }

            if room.clients.len() >= room.size.into()
            {
                return self.send_error_packet(format!("The room is full."));
            }

            relay.hosts.insert(self.sender.clone(), room.clients[0].sender.clone());

            room.clients.push(self.clone());
            room.clients[0].send_packet(TransmitPacket::JoinRoom)?;

            return self.send_packet(TransmitPacket::JoinRoom);
        }
        
        return self.send_error_packet(format!("The room '{}' does not exist.", id));  
    }

    fn send_packet(&self, packet: TransmitPacket) -> Result<()>
    {
        let serialized_packet = serde_json::to_string(&packet).unwrap();

        self.sender.send(Message::Text(serialized_packet))
    }

    fn send_error_packet(&self, message: String) -> Result<()>
    {
        let error_packet = TransmitPacket::Error { message };
        let serialized_error_packet = serde_json::to_string(&error_packet).unwrap();

        self.sender.send(Message::Text(serialized_error_packet))
    }
}

impl Handler for Client
{
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
                    }
                }
            }
        }
        else if message.is_binary()
        {
            let relay = get_relay!(self);

            if let Some(host) = relay.hosts.get(&self.sender) 
            {
                return host.send(message);
            }
            else if let Some(room_id) = self.room.clone()
            {
                if let Some(room) = relay.rooms.get(&room_id) 
                {
                    let data = message.into_data();
                    if data.len() > 0
                    {
                        let index = data[0];
                        let message = Message::Binary(data[1..].to_vec());

                        if index > 0 && usize::from(index) < room.clients.len()
                        {
                            return room.clients[usize::from(index)].sender.send(message);
                        }
                        else if index == 0
                        {
                            for client in &room.clients 
                            {   
                                if client.sender == self.sender 
                                {
                                    continue;
                                }
        
                                client.sender.send(message.clone())?;                            
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    } 
}