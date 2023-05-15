use std::net::{SocketAddr};
use std::sync::{Arc};

use uuid::{Uuid};

use tokio::net::{TcpStream};
use tokio::sync::{Mutex};
use tokio_tungstenite::{accept_async, tungstenite::{Message, Error}, WebSocketStream};

use futures_util::{SinkExt, StreamExt};
use dashmap::{DashMap};

use serde::{Deserialize, Serialize};

const DEFAULT_ROOM_SIZE: u8 = 2;

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

struct Client 
{
    socket_addr: SocketAddr,
    websocket_stream: Mutex<WebSocketStream<TcpStream>>,
}

impl Client 
{
    pub fn new(socket_addr: SocketAddr, websocket_stream: WebSocketStream<TcpStream>) -> Arc<Client>
    {
        Arc::new(
            Client 
            {
                socket_addr,
                websocket_stream: Mutex::new(websocket_stream),
            }
        )
    }

    pub async fn next_message(&self) -> Option<Result<Message, Error>>
    {
        let mut websocket_stream = self.websocket_stream.lock().await;

        websocket_stream.next().await
    }

    pub async fn send_message(&self, message: Message) -> Result<(), Error> 
    {
        let mut websocket_stream = self.websocket_stream.lock().await;

        websocket_stream.send(message).await
    }
}

struct Room 
{
    id: String,
    users: Vec<Arc<Client>>,
    size: u8,
}

impl Room 
{
    fn new(size: u8) -> Room
    {
        return Room {
            id: Uuid::new_v4().to_string(),
            users: vec![],
            size,
        };
    }

    fn add(&mut self, client: Arc<Client>) 
    {
        self.users.push(client);
    }

    fn length(&mut self) -> usize 
    {
        return self.users.len();
    }

    fn remove(&mut self, client: Arc<Client>) 
    {
        self.users.retain(|x| x.socket_addr != client.socket_addr);
    }
}

pub struct Relay 
{
    rooms: dashmap::DashMap<String, Room>,
    sockets: dashmap::DashMap<SocketAddr, String>,
}

impl Relay 
{
    pub fn new() -> Relay 
    {
        Relay {
            rooms: DashMap::new(),
            sockets: DashMap::new(),
        }
    }

    pub async fn handle_connection(&self, tcp_stream: TcpStream, socket_addr: SocketAddr)
    {
        match accept_async(tcp_stream).await
        {
            Ok(websocket_stream) => 
            {
                let client = Client::new(socket_addr, websocket_stream);
    
                while let Some(message) = client.next_message().await
                {
                    match message
                    {
                        Ok(message) => 
                        {
                            self.handle_message(client.clone(), message).await;
                        },
                        Err(error) => 
                        {
                            println!("Invalid message provided: {}", error);
                        },
                    }
                }
            },

            Err(error) => 
            {
                println!("Failed to obtain websocket stream: {}", error);
            },
        }
        
    }

    async fn handle_message(&self, client: Arc<Client>, message: Message)
    {
        if message.is_text()
        {
            if let Some(socket) = self.sockets.get(&client.socket_addr) 
            {
                if let Some(room) = self.rooms.get(socket.value()) 
                {
                    client.send_message(Message::Text(room.id.clone())).await;
                    return;
                }
            }

            if let Ok(text) = message.into_text()
            {
                if let Ok(packet) = serde_json::from_str(&text) 
                {
                    match packet {
                        ReceivePacket::CreateRoom { size } => self.handle_create_room(client, size).await,
                        ReceivePacket::JoinRoom { id } => self.handle_join_room(client, id).await,
                    }
                }
            }
        }
    }

    async fn handle_create_room(&self, client: Arc<Client>, size_option: Option<u8>)
    {
        let size = size_option.unwrap_or(DEFAULT_ROOM_SIZE);
        if size == 0 
        {
            return self.send_error_packet(client, format!("The size value of '{}' is not valid.", size)).await;   
        }

        let mut room = Room::new(size);
        room.add(client.clone());

        let packet = TransmitPacket::CreateRoom { id: room.id.clone() };

        self.sockets.insert(client.socket_addr, room.id.clone());
        self.rooms.insert(room.id.clone(), room);

        self.send_packet(client, packet).await;
    }

    async fn handle_join_room(&self, client: Arc<Client>, id: String)
    {
        if let Some(mut room) = self.rooms.get_mut(&id) 
        {
            if room.length() >= room.size.into()
            {
                return self.send_error_packet(client, format!("The room is full.")).await;
            }

            self.sockets.insert(client.socket_addr, room.id.clone());
            room.add(client.clone());

            self.send_packet(client, TransmitPacket::JoinRoom).await;
        }
        else 
        {
            return self.send_error_packet(client, format!("The room '{}' does not exist.", id)).await;  
        }
    }

    async fn send_packet(&self, client: Arc<Client>, packet: TransmitPacket)
    {
        let serialized_packet = serde_json::to_string(&packet).unwrap();

        if let Err(error) = client.send_message(Message::Text(serialized_packet)).await 
        {
            println!("Failed to send packet: {}", error);
        } 
    }

    async fn send_error_packet(&self, client: Arc<Client>, message: String)
    {
        let error_packet = TransmitPacket::Error { message };
        let serialized_error_packet = serde_json::to_string(&error_packet).unwrap();

        if let Err(error) = client.send_message(Message::Text(serialized_error_packet)).await 
        {
            println!("Failed to send error packet: {}", error);
        }
    }

}