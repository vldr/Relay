use std::net::{SocketAddr};
use std::str::FromStr;
use std::sync::{Arc};

use uuid::{Uuid};

use tokio::net::{TcpStream};
use tokio::sync::{Mutex};
use tokio_tungstenite::{accept_async, tungstenite::{Message, Error}, WebSocketStream};

use futures_util::{SinkExt, StreamExt};
use dashmap::{DashMap};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Packet {
    CreateRoom { size: Option<u64> },
    JoinRoom { id: String },
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
    size: u64,
    users: Vec<Arc<Client>> 
}

impl Room 
{
    fn new(size: u64) -> Uuid
    {
        return Uuid::new_v4();
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
}

impl Relay 
{
    pub fn new() -> Relay 
    {
        Relay {
            rooms: DashMap::new(),
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
                            self.handle_message(&client, message).await;
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

    async fn handle_message(&self, client: &Client, message: Message)
    {
        if message.is_text()
        {
            if let Ok(text) = message.into_text()
            {
                if let Ok(packet) = serde_json::from_str(&text) 
                {
                    match packet {
                        Packet::CreateRoom { size } => self.handle_create_room(client, size),
                        Packet::JoinRoom { id } => self.handle_join_room(client, id),
                    }
                }
            }
        }
    }

    fn handle_create_room(&self, client: &Client, size: Option<u64>)
    {
        println!("CreateRoom {:?}", size);
    }

    fn handle_join_room(&self, client: &Client, id: String)
    {
        println!("Join Room {:?}", id);
    }
}