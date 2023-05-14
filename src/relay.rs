use tokio::net::{TcpStream};
use tokio::sync::Mutex;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{accept_async, tungstenite::{Message, Error}, WebSocketStream};

use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;

struct Client 
{
    socket_addr: SocketAddr,
    websocket_stream: Arc<Mutex<WebSocketStream<TcpStream>>>,
}

impl Client 
{
    fn new(socket_addr: SocketAddr, websocket_stream: WebSocketStream<TcpStream>) -> Arc<Client>
    {
        Arc::new(
            Client {
                socket_addr,
                websocket_stream: Arc::new(Mutex::new(websocket_stream)),
            }
        )
    }

    async fn next_message(&self) -> Option<Result<Message, Error>>
    {
        let mut websocket_stream = self.websocket_stream.lock().await;

        websocket_stream.next().await
    }

    async fn send_message(&self, message: Message) -> Result<(), Error> 
    {
        let mut websocket_stream = self.websocket_stream.lock().await;

        websocket_stream.send(message).await
    }
}

struct Room 
{
    size: u64,
    clients: Vec<Arc<Client>> 
}

pub struct Relay 
{
    rooms: dashmap::DashMap<String, Room>,
} 

impl Relay {
    pub fn new() ->  Relay 
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
                            println!("Invalid message provided {}", error);
                            break;
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
            client.send_message(message).await;
        }
    }
}