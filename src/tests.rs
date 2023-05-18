#[cfg(test)]
mod tests 
{
    use crate::relay::{Server, Client, TransmitPacket, ReceivePacket};

    use ws::{Builder, Settings};
    use std::net::{SocketAddr};
    use tungstenite::{connect, Message};
    use std::thread::{spawn};
    use std::sync::{mpsc};

    macro_rules! create_socket {
        ($value:expr) => {
            {
                let (socket, _) = connect(format!("ws://{}", $value)).unwrap();
                socket
            }
        };
    }

    macro_rules! write_binary_message {
        ($value:expr, $value2:expr) => {
            $value.write_message(Message::Binary($value2)).unwrap()
        };
    }

    macro_rules! write_message {
        ($value:expr, $value2:expr) => {
            let packet = $value2;
            let serialized_packet = serde_json::to_string(&packet).unwrap();
            $value.write_message(Message::Text(serialized_packet)).unwrap()
        };
    }

    macro_rules! read_message {
        ($value:expr, $pattern:pat => $extracted_value:expr) => {
            match serde_json::from_str(&$value.read_message().unwrap().clone().into_text().unwrap()).unwrap() {
                $pattern => $extracted_value,
                unknown => panic!("pattern doesn't match: {:?}", unknown),
            }
        };
    }

    macro_rules! read_binary_message {
        ($value:expr) => {
            $value.read_message().unwrap().clone().into_data()
        };
    }

    fn setup() -> SocketAddr
    {
        let (tx, rx) = mpsc::channel();

        spawn(move || {
            let relay = Server::new();
            let ws = Builder::new()
                .with_settings(Settings {
                    max_connections: 256,
                    ..Settings::default()
                })
                .build(|sender| Client::new(&relay, sender))
                .expect("Failed to build test WebSocket server.")
                .bind("127.0.0.1:0")
                .expect("Failed to bind test WebSocket server.");

            tx.send(ws.local_addr().unwrap()).unwrap();
            
            ws.run().expect("Failed to start test WebSocket server.");
        }); 

        return rx.recv().unwrap()
    }

    #[test]
    fn errors() 
    {
        //
        // Setup test.
        //

        let socket_addr = setup();

        //
        // Test sending an invalid packet
        //

        let mut host_socket = create_socket!(socket_addr);
        
        write_message!(host_socket, String::new());
        read_message!(host_socket, TransmitPacket::Error { message } => assert_eq!("The following packet is invalid: \"\"", message));

        //
        // Test sending a binary packet while not in a room.
        //

        write_binary_message!(host_socket, vec![]);
        read_message!(host_socket, TransmitPacket::Error { message } => assert_eq!("You're not currently in a room.", message));
        
        //
        // Test creating an invalid room.
        //

        write_message!(host_socket, ReceivePacket::CreateRoom { size: Some(0) });
        read_message!(host_socket, TransmitPacket::Error { message } => assert_eq!("You cannot create an empty room.", message));

        //
        // Test creating a valid room.
        //  

        write_message!(host_socket, ReceivePacket::CreateRoom { size: None });

        let room_id = read_message!(host_socket, TransmitPacket::CreateRoom { id } => id);

        //
        // Test joining another room as host.
        //

        write_message!(host_socket, ReceivePacket::JoinRoom { id: String::new() });
        read_message!(host_socket, TransmitPacket::Error { message } => assert_eq!("You're already in a room.", message));

        //
        // Test joining an non-existant room.
        //
        
        let mut client_socket = create_socket!(socket_addr);
        
        write_message!(client_socket, ReceivePacket::JoinRoom { id: String::new() });
        read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!("The room '' does not exist.", message));

        //
        // Test joining the room as a client.
        //

        write_message!(client_socket, ReceivePacket::JoinRoom { id: room_id.clone() });

        read_message!(client_socket, TransmitPacket::JoinRoom => ());
        read_message!(host_socket, TransmitPacket::JoinRoom => ());

        //
        // Test joining another room as a client.
        //

        write_message!(client_socket, ReceivePacket::JoinRoom { id: String::new() });
        read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!("You're already in a room.", message));

        //
        // Test creating a room while in a room.
        // 

        write_message!(host_socket, ReceivePacket::CreateRoom { size: None });
        read_message!(host_socket, TransmitPacket::Error { message } => assert_eq!("You're currently in a room.", message));

        write_message!(client_socket, ReceivePacket::CreateRoom { size: None });
        read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!("You're currently in a room.", message));

        //
        // Test joining a full room.
        //

        let mut client_socket_2 = create_socket!(socket_addr);
        
        write_message!(client_socket_2, ReceivePacket::JoinRoom { id: room_id.clone() });
        read_message!(client_socket_2, TransmitPacket::Error { message } => assert_eq!("The room is full.", message));  
    }

    #[test]
    fn communication() 
    {
        //
        // The number of clients to test with.
        //

        const N: u8 = 255;

        //
        // Setup test.
        //

        let socket_addr = setup();

        //
        // Create a room of size N.
        //

        let mut host_socket = create_socket!(socket_addr);

        write_message!(host_socket, ReceivePacket::CreateRoom { size: Some(N) } );
        let room_id = read_message!(host_socket, TransmitPacket::CreateRoom { id } => id);

        //
        // Create N clients.
        //

        let mut client_sockets = vec![];
        for _ in 0 .. N - 1
        {
            let mut client_socket = create_socket!(socket_addr);

            write_message!(client_socket, ReceivePacket::JoinRoom { id: room_id.clone() } );

            read_message!(host_socket, TransmitPacket::JoinRoom => ());
            read_message!(client_socket, TransmitPacket::JoinRoom => ());

            client_sockets.push(client_socket);
        }       
        
        //
        // Test room bounds.
        //

        let mut client_socket = create_socket!(socket_addr);

        write_message!(client_socket, ReceivePacket::JoinRoom { id: room_id.clone() } );
        read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!("The room is full.", message));  

        client_socket.close(None).unwrap();

        //
        // Test broadcasting.
        //

        write_binary_message!(host_socket, vec![0]);
        write_binary_message!(host_socket, vec![0, 1, 2]);

        for client_socket in client_sockets.iter_mut()
        {
            let data = read_binary_message!(client_socket);
            assert_eq!(data.len(), 0);

            let data = read_binary_message!(client_socket);
            assert_eq!(data.len(), 2);
            assert_eq!(data[0], 1);
            assert_eq!(data[1], 2);
        }

        //
        // Test sending to clients.
        //

        for (index, client_socket) in client_sockets.iter_mut().enumerate()
        {
            write_binary_message!(host_socket, vec![index.try_into().unwrap()]);
            write_binary_message!(host_socket, vec![index.try_into().unwrap(), 1, 2]);

            let data = read_binary_message!(client_socket);
            assert_eq!(data.len(), 0);

            let data = read_binary_message!(client_socket);
            assert_eq!(data.len(), 2);
            assert_eq!(data[0], 1);
            assert_eq!(data[1], 2);
        }

        //
        // Test sending to host.
        //

        for (index, client_socket) in client_sockets.iter_mut().enumerate()
        {
            write_binary_message!(client_socket, vec![]);
            write_binary_message!(client_socket, vec![1, 2]);

            let data = read_binary_message!(host_socket);
            assert_eq!(data.len(), 1);
            assert_eq!(data[0], <usize as TryInto<u8>>::try_into(index + 1).unwrap());

            let data = read_binary_message!(host_socket);
            assert_eq!(data.len(), 3);
            assert_eq!(data[0], <usize as TryInto<u8>>::try_into(index + 1).unwrap());
            assert_eq!(data[1], 1);
            assert_eq!(data[2], 2);
        }

        //
        // Close host and N clients.
        //
        
        let size = client_sockets.len();
        for (expected_index, client_socket) in client_sockets.iter_mut().rev().enumerate()
        {
            client_socket.close(None).unwrap();
            read_message!(host_socket, TransmitPacket::LeaveRoom { index } => assert_eq!(size - expected_index, index));
        }

        host_socket.close(None).unwrap();
    }

    #[test]
    fn rooms() 
    {
        //
        // Setup test.
        //

        let socket_addr = setup();

        for stage in ["client_close", "host_close", "client_leave_packet", "host_leave_packet"] 
        {
            //
            // Test creating a valid room.
            // 
            
            let mut host_socket = create_socket!(socket_addr);
            write_message!(host_socket, ReceivePacket::CreateRoom { size: None });

            let room_id = read_message!(host_socket, TransmitPacket::CreateRoom { id } => id);

            //
            // Test joining the room as a client.
            //

            let mut client_socket = create_socket!(socket_addr);

            write_message!(client_socket, ReceivePacket::JoinRoom { id: room_id.clone() });

            read_message!(client_socket, TransmitPacket::JoinRoom => ());
            read_message!(host_socket, TransmitPacket::JoinRoom => ());

            //
            // Test leaving room.
            //
            
            match stage {
                "client_close" => 
                {   
                    client_socket.close(None).unwrap();
                    read_message!(host_socket, TransmitPacket::LeaveRoom { index } => assert_eq!(1, index));

                },
                "host_close" => 
                {
                    host_socket.close(None).unwrap();
                    read_message!(client_socket, TransmitPacket::Kick { message } => assert_eq!("The host has left the room.", message));

                    write_message!(client_socket, ReceivePacket::JoinRoom { id: room_id.clone() });
                    read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!(format!("The room '{}' does not exist.", room_id.clone()), message));

                    write_message!(client_socket, ReceivePacket::CreateRoom { size: None });
                    read_message!(client_socket, TransmitPacket::CreateRoom { id } => id);
                },
                "client_leave_packet" => 
                {
                    write_message!(client_socket, ReceivePacket::LeaveRoom);
                    read_message!(host_socket, TransmitPacket::LeaveRoom { index } => assert_eq!(1, index));
                },
                "host_leave_packet" => 
                {
                    write_message!(host_socket, ReceivePacket::LeaveRoom);
                    read_message!(client_socket, TransmitPacket::Kick { message } => assert_eq!("The host has left the room.", message));

                    write_message!(client_socket, ReceivePacket::JoinRoom { id: room_id.clone() });
                    read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!(format!("The room '{}' does not exist.", room_id.clone()), message));

                    write_message!(client_socket, ReceivePacket::CreateRoom { size: None });
                    read_message!(client_socket, TransmitPacket::CreateRoom { id } => id);
                },
                _ => panic!("invalid stage {}", stage),
            }
        }
    }
}