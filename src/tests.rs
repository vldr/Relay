#[cfg(test)]
mod tests 
{
    use crate::relay::{Server, Client, TransmitPacket, ReceivePacket};

    use ws::{Builder, Settings};
    use tungstenite::{connect, Message};
    use std::thread::{spawn};
  
    const ADDR_SPEC: &str = "127.0.0.1:3012";

    macro_rules! create_socket {
        () => {
            {
                let (socket, _) = connect(format!("ws://{}", ADDR_SPEC)).unwrap();
                socket
            }
        };
    }

    macro_rules! write_message {
        ($value:expr, $value2:expr) => {
            $value.write_message(Message::from_receive_packet($value2)).unwrap()
        };
    }

    macro_rules! read_message {
        ($value:expr, $pattern:pat => $extracted_value:expr) => {
            match $value.read_message().unwrap().into_transmit_packet() {
                $pattern => $extracted_value,
                _ => panic!("pattern doesn't match!"),
            }
        };
    }
    
    trait PacketMessage 
    {
        fn from_transmit_packet(packet: TransmitPacket) -> Message;
        fn from_receive_packet(packet: ReceivePacket) -> Message;

        fn into_transmit_packet(&self) -> TransmitPacket;
        fn into_receive_packet(&self) -> ReceivePacket;
    }

    impl PacketMessage for Message 
    {
        fn from_transmit_packet(packet: TransmitPacket) -> Message
        {
            let serialized_packet = serde_json::to_string(&packet).unwrap();

            Message::Text(serialized_packet)
        }

        fn from_receive_packet( packet: ReceivePacket) -> Message
        {
            let serialized_packet = serde_json::to_string(&packet).unwrap();

            Message::Text(serialized_packet)
        }

        fn into_transmit_packet(&self) -> TransmitPacket
        {
            let text = self.clone().into_text().unwrap();
            serde_json::from_str(&text).unwrap()
        }

        fn into_receive_packet(&self) -> ReceivePacket
        {
            let text = self.clone().into_text().unwrap();
            serde_json::from_str(&text).unwrap()
        }
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
        // Test creating an invalid room.
        //

        let mut host_socket = create_socket!();

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
        // Test joining the room as a client.
        //

        let mut client_socket = create_socket!();

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

        write_message!(client_socket, ReceivePacket::CreateRoom { size: None });
        read_message!(client_socket, TransmitPacket::Error { message } => assert_eq!("You're currently in a room.", message));
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