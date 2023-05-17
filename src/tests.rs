#[cfg(test)]
mod tests 
{
    use crate::relay::{Server, Client, TransmitPacket, ReceivePacket};

    use ws::{Builder, Settings};
    use tungstenite::{connect, Message};
    use std::thread::{spawn};
  
    const ADDR_SPEC: &str = "localhost:3012";

    macro_rules! create_socket {
        () => {
            {
                let (socket, _) = connect(format!("ws://{}", ADDR_SPEC)).unwrap();
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
                _ => panic!("pattern doesn't match!"),
            }
        };
    }

    macro_rules! read_binary_message {
        ($value:expr) => {
            $value.read_message().unwrap().clone().into_data()
        };
    }

    fn setup()
    {
        spawn(|| {
            let relay = Server::new();
            let ws = Builder::new()
                .with_settings(Settings::default())
                .build(|sender| Client::new(relay.clone(), sender))
                .expect("Failed to build test WebSocket server.");

            ws.listen(ADDR_SPEC).expect("Failed to start test WebSocket server.");
        });   
    }

    #[test]
    fn test_errors() 
    {
        //
        // Setup test.
        //

        setup();

        //
        // Test sending an invalid packet
        //

        let mut host_socket = create_socket!();
        
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
        
        let mut client_socket = create_socket!();
        
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

        let mut client_socket_2 = create_socket!();
        
        write_message!(client_socket_2, ReceivePacket::JoinRoom { id: room_id.clone() });
        read_message!(client_socket_2, TransmitPacket::Error { message } => assert_eq!("The room is full.", message));

    }

    #[test]
    fn test_create_join_leave_room() 
    {
        //
        // Setup test.
        //

        setup();

        for stage in ["client_close", "host_close", "client_leave_packet", "host_leave_packet"] 
        {
            //
            // Test creating a valid room.
            // 
            
            let mut host_socket = create_socket!();
            write_message!(host_socket, ReceivePacket::CreateRoom { size: None });

            let room_id = read_message!(host_socket, TransmitPacket::CreateRoom { id } => id);

            //
            // Test joining the room as a client.
            //

            let mut client_socket = create_socket!();

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