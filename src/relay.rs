use flume::{Sender, SendError};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::protocol::Message;
use tungstenite::{
    handshake::server::{Request, Response},
    http::{StatusCode, Uri},
};
use uuid::Uuid;


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

trait PacketSender {
    fn send_packet(&self, packet: ResponsePacket) -> Result<(), SendError<Message>>;
    fn send_error_packet(&self, message: String) -> Result<(), SendError<Message>>;
}

impl PacketSender for Sender<Message> {
    fn send_packet(&self, packet: ResponsePacket) -> Result<(), SendError<Message>> {
        let serialized_packet = serde_json::to_string(&packet).unwrap();

        self.send(Message::Text(serialized_packet))
    }

    fn send_error_packet(&self, message: String) -> Result<(), SendError<Message>> {
        let error_packet = ResponsePacket::Error { message };

        self.send_packet(error_packet)
    }
}

struct Room {
    size: usize,
    senders: Vec<Sender<Message>>,
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
            let (sender, receiver) = flume::unbounded();
            let (outgoing, incoming) = websocket_stream.split();

            let mut client = Client::new(sender.clone());

            let incoming_handler = incoming.try_for_each(|message| {
                if let Err(error) = client.handle_message(&server, message) {
                    println!("Failed to handle message: {}", error);
                }

                future::ok(())
            });

            let outgoing_handler = receiver.stream().map(Ok).forward(outgoing);

            pin_mut!(incoming_handler, outgoing_handler);
            future::select(incoming_handler, outgoing_handler).await;

            if let Err(error) = client.handle_close(&server) {
                println!("Failed to handle close: {}", error);
            }
        }
    }
}

pub struct Client {
    sender: Sender<Message>,
    room_id: Option<String>,
}

impl Client {
    pub fn new(sender: Sender<Message>) -> Client {
        Client {
            sender,
            room_id: None,
        }
    }

    fn handle_create_room(&mut self, server: &RwLock<Server>, size_option: Option<usize>) -> Result<(), SendError<Message>> {
        let mut server = server.write().unwrap();

        if server.rooms.iter().any(|(_, room)| {
            room.senders
                .iter()
                .any(|sender| sender.same_channel(&self.sender))
        }) {
            return Ok(());
        }

        let size = size_option.unwrap_or(Room::DEFAULT_ROOM_SIZE);
        if size == Room::MIN_ROOM_SIZE || size >= Room::MAX_ROOM_SIZE {
            return self
                .sender
                .send_error_packet("The room size is not valid".to_string());
        }

        let room_id = Uuid::new_v4().to_string();
        if server.rooms.contains_key(&room_id) {
            return self
                .sender
                .send_error_packet("A room with that identifier already exists.".to_string());
        }

        let mut room = Room::new(size);
        room.senders.push(self.sender.clone());

        server.rooms.insert(room_id.clone(), room);

        self.room_id = Some(room_id.clone());
        self.sender
            .send_packet(ResponsePacket::Create { id: room_id })
    }

    fn handle_join_room(&mut self, server: &RwLock<Server>, room_id: String) -> Result<(), SendError<Message>> {
        let mut server = server.write().unwrap();

        if server.rooms.iter().any(|(_, room)| {
            room.senders
                .iter()
                .any(|sender| sender.same_channel(&self.sender))
        }) {
            return Ok(());
        }

        let Some(room) = server.rooms.get_mut(&room_id) else {
            return self.sender.send_error_packet("The room does not exist.".to_string()); 
        };

        if room.senders.len() >= room.size {
            return self
                .sender
                .send_error_packet("The room is full.".to_string());
        }

        room.senders.push(self.sender.clone());

        for sender in &room.senders {
            if sender.same_channel(&self.sender) {
                sender.send_packet(ResponsePacket::Join {
                    size: Some(room.senders.len() - 1),
                })?;
            } else {
                sender.send_packet(ResponsePacket::Join { size: None })?;
            }
        }

        self.room_id = Some(room_id);

        Ok(())
    }

    fn handle_leave_room(&mut self, server: &RwLock<Server>) -> Result<(), SendError<Message>> {
        let mut server = server.write().unwrap();

        let Some(room_id) = &self.room_id else {
            return Ok(());
        };

        let Some(room) = server.rooms.get_mut(room_id) else {
            return Ok(());
        };

        let Some(index) = room.senders.iter().position(|sender| sender.same_channel(&self.sender)) else {
            return Ok(());
        };

        room.senders.remove(index);

        for sender in &room.senders {
            sender.send_packet(ResponsePacket::Leave { index })?;
        }

        if room.senders.is_empty() {
            server.rooms.remove(room_id);
        }

        self.room_id = None;

        Ok(())
    }

    fn handle_message(&mut self, server: &RwLock<Server>, message: Message) -> Result<(), SendError<Message>> {
        if message.is_text() {
            let Ok(text) = message.into_text() else {
                return Ok(())
            };

            let Ok(packet) = serde_json::from_str(&text) else {
                return Ok(())
            };

            return match packet {
                RequestPacket::Create { size } => self.handle_create_room(server, size),
                RequestPacket::Join { id } => self.handle_join_room(server, id),
                RequestPacket::Leave => self.handle_leave_room(server),
            };
        } else if message.is_binary() {
            let server = server.read().unwrap();

            let Some(room_id) = &self.room_id else {
                return Ok(());
            };

            let Some(room) = server.rooms.get(room_id) else {
                return Ok(());
            };

            let Some(index) = room.senders.iter().position(|sender| sender.same_channel(&self.sender)) else {
                return Ok(());
            };

            let mut data = message.into_data();
            if data.is_empty() {
                return Ok(());
            }

            let source = u8::try_from(index).unwrap();
            let destination = usize::from(data[0]);

            data[0] = source;

            if destination < room.senders.len() {
                return room.senders[destination].send(Message::Binary(data));
            } else if destination == usize::from(u8::MAX) {
                for sender in &room.senders {
                    if sender.same_channel(&self.sender) {
                        continue;
                    }

                    sender.send(Message::Binary(data.clone()))?;
                }
            }
        }

        Ok(())
    }

    fn handle_close(&mut self, server: &RwLock<Server>) -> Result<(), SendError<Message>> {
        self.handle_leave_room(server)
    }
}
