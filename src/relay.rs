use futures_util::{future::join_all, stream::SplitSink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, vec};
use tokio::{net::TcpStream, sync::Mutex, sync::RwLock};
use tokio_tungstenite::{tungstenite::protocol::Message, WebSocketStream};
use tungstenite::{
    handshake::server::{Request, Response},
    http::{StatusCode, Uri},
};
use uuid::Uuid;

type Sender = Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RequestPacket {
    Join { id: String },
    Create { size: Option<usize> },
    Leave,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResponsePacket {
    Join {
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<usize>,
    },
    Create {
        id: String,
    },
    Leave {
        index: usize,
    },
    Error {
        message: String,
    },
}

struct Room {
    size: usize,
    senders: Vec<Sender>,
}

impl Room {
    const MIN_ROOM_SIZE: usize = 0;
    const MAX_ROOM_SIZE: usize = 255;
    const DEFAULT_ROOM_SIZE: usize = 2;

    fn new(size: usize) -> Room {
        Room {
            senders: Vec::new(),
            size,
        }
    }
}

pub struct Server {
    rooms: HashMap<String, Room>,
}

impl Server {
    pub fn new() -> Arc<RwLock<Server>> {
        Arc::new(RwLock::new(Server {
            rooms: HashMap::new(),
        }))
    }

    pub async fn handle_connection(
        tcp_stream: TcpStream,
        server: Arc<RwLock<Server>>,
        host: String,
    ) {
        let callback = |request: &Request, response: Response| {
            if host.is_empty() {
                return Ok(response);
            }

            let Some(header_value) = request.headers().get("Origin") else {
                let response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(None)
                    .unwrap();

                return Err(response);
            };

            let Ok(origin) = header_value.to_str() else {
                let response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(None)
                    .unwrap();

                return Err(response);
            };

            let Ok(origin_uri) = origin.parse::<Uri>() else {
                let response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(None)
                    .unwrap();
                
                return Err(response);
            };

            let Some(origin_host) = origin_uri.host() else {
                let response = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(None)
                    .unwrap();

                return Err(response);
            };

            if origin_host != host && !origin_host.ends_with(format!(".{}", host).as_str()) {
                let response = Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(None)
                    .unwrap();

                return Err(response);
            }

            Ok(response)
        };

        if let Ok(websocket_stream) =
            tokio_tungstenite::accept_hdr_async(tcp_stream, callback).await
        {
            let (sender, mut receiver) = websocket_stream.split();
            let sender = Arc::new(Mutex::new(sender));

            let mut client = Client::new(sender.clone());

            while let Some(message) = receiver.next().await {
                match message {
                    Ok(message) => client.handle_message(&server, message).await,
                    Err(error) => {
                        println!("Failed to read message: {}", error);
                        break;
                    }
                }
            }

            client.handle_close(&server).await
        }
    }
}

pub struct Client {
    sender: Sender,
    room_id: Option<String>,
}

impl Client {
    pub fn new(sender: Sender) -> Client {
        Client {
            sender,
            room_id: None,
        }
    }

    async fn send(&self, sender: &Sender, message: Message) {
        let mut sender = sender.lock().await;
        if let Err(error) = sender.send(message).await {
            println!("Failed to send: {}", error);
        }
    }

    async fn send_packet(&self, sender: &Sender, packet: ResponsePacket) {
        let serialized_packet = serde_json::to_string(&packet).unwrap();

        self.send(sender, Message::Text(serialized_packet)).await;
    }

    async fn send_error_packet(&self, sender: &Sender, message: String) {
        let error_packet = ResponsePacket::Error { message };

        self.send_packet(sender, error_packet).await
    }

    async fn handle_create_room(&mut self, server: &RwLock<Server>, size_option: Option<usize>) {
        let mut server = server.write().await;

        if server.rooms.iter().any(|(_, room)| {
            room.senders
                .iter()
                .any(|sender| Arc::ptr_eq(sender, &self.sender))
        }) {
            return;
        }

        let size = size_option.unwrap_or(Room::DEFAULT_ROOM_SIZE);
        if size == Room::MIN_ROOM_SIZE || size >= Room::MAX_ROOM_SIZE {
            drop(server);

            return self
                .send_error_packet(&self.sender, "The room size is not valid".to_string())
                .await;
        }

        let room_id = Uuid::new_v4().to_string();
        if server.rooms.contains_key(&room_id) {
            drop(server);

            return self
                .send_error_packet(
                    &self.sender,
                    "A room with that identifier already exists.".to_string(),
                )
                .await;
        }

        let mut room = Room::new(size);
        room.senders.push(self.sender.clone());

        server.rooms.insert(room_id.clone(), room);
        self.room_id = Some(room_id.clone());

        drop(server);

        self.send_packet(&self.sender, ResponsePacket::Create { id: room_id })
            .await
    }

    async fn handle_join_room(&mut self, server: &RwLock<Server>, room_id: String) {
        let mut server = server.write().await;

        if server.rooms.iter().any(|(_, room)| {
            room.senders
                .iter()
                .any(|sender| Arc::ptr_eq(sender, &self.sender))
        }) {
            return;
        }

        let Some(room) = server.rooms.get_mut(&room_id) else {
            drop(server);

            return self.send_error_packet(&self.sender, "The room does not exist.".to_string()).await; 
        };

        if room.senders.len() >= room.size {
            drop(server);

            return self
                .send_error_packet(&self.sender, "The room is full.".to_string())
                .await;
        }

        room.senders.push(self.sender.clone());

        let senders = room.senders.clone();
        let size = Some(room.senders.len() - 1);

        drop(server);

        self.room_id = Some(room_id);

        let mut futures = vec![];
        for sender in &senders {
            if Arc::ptr_eq(sender, &self.sender) {
                futures.push(self.send_packet(sender, ResponsePacket::Join { size }));
            } else {
                futures.push(self.send_packet(sender, ResponsePacket::Join { size: None }));
            }
        }

        join_all(futures).await;
    }

    async fn handle_leave_room(&mut self, server: &RwLock<Server>) {
        let mut server = server.write().await;

        let Some(room_id) = &self.room_id else {
            return;
        };

        let Some(room) = server.rooms.get_mut(room_id) else {
            return;
        };

        let Some(index) = room.senders.iter().position(|sender| Arc::ptr_eq(sender, &self.sender)) else {
            return;
        };

        room.senders.remove(index);

        let senders = room.senders.clone();

        if room.senders.is_empty() {
            server.rooms.remove(room_id);
        }

        self.room_id = None;

        drop(server);

        let mut futures = vec![];
        for sender in &senders {
            futures.push(self.send_packet(sender, ResponsePacket::Leave { index }));
        }

        join_all(futures).await;
    }

    async fn handle_message(&mut self, server: &RwLock<Server>, message: Message) {
        if message.is_text() {
            let Ok(text) = message.into_text() else {
                return
            };

            let Ok(packet) = serde_json::from_str(&text) else {
                return
            };

            match packet {
                RequestPacket::Create { size } => self.handle_create_room(server, size).await,
                RequestPacket::Join { id } => self.handle_join_room(server, id).await,
                RequestPacket::Leave => self.handle_leave_room(server).await,
            }
        } else if message.is_binary() {
            let server = server.read().await;

            let Some(room_id) = &self.room_id else {
                return;
            };

            let Some(room) = server.rooms.get(room_id) else {
                return;
            };

            let Some(index) = room.senders.iter().position(|sender| Arc::ptr_eq(sender, &self.sender)) else {
                return;
            };

            let mut data = message.into_data();
            if data.is_empty() {
                return;
            }

            let source = u8::try_from(index).unwrap();
            let destination = usize::from(data[0]);

            data[0] = source;

            if destination < room.senders.len() {
                let sender = room.senders[destination].clone();

                drop(server);

                return self.send(&sender, Message::Binary(data)).await;
            } else if destination == usize::from(u8::MAX) {
                let senders = room.senders.clone();

                drop(server);

                let mut futures = vec![];
                for sender in &senders {
                    if Arc::ptr_eq(sender, &self.sender) {
                        continue;
                    }

                    futures.push(self.send(sender, Message::Binary(data.clone())));
                }

                join_all(futures).await;
            }
        }
    }

    async fn handle_close(&mut self, server: &RwLock<Server>) {
        self.handle_leave_room(server).await
    }
}
