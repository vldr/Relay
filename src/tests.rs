#[cfg(test)]
mod tests 
{
    use crate::relay::{Server, ServerPacket, ClientPacket};

    use std::net::{SocketAddr};
    use tokio::{net::{TcpListener}};
    use tungstenite::{Message, connect};

    macro_rules! create_socket {
        ($value:expr) => {
            {
                let (socket, _) = connect(format!("ws://{}", $value)).unwrap();
                socket
            }
        };
    }

    macro_rules! close_socket {
        ($value:expr) => {
            {
                $value.close(None).unwrap();
                loop {
                    if $value.read_message().is_err() {
                        break;
                    }
                }
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

    async fn setup() -> SocketAddr
    {
        let listener = TcpListener::bind("127.0.0.1:0").await
            .expect("Failed to bind");
    
        let server = Server::new();
        let socket_addr = listener.local_addr().unwrap();

        tokio::spawn(async move    
        {
            while let Ok((tcp_stream, _)) = listener.accept().await 
            {
                tokio::spawn(Server::handle_connection(server.clone(), tcp_stream));
            }
        });

        return socket_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn errors() 
    {
        //
        // Setup test.
        //

        let socket_addr = setup().await;

        //
        // Test creating an invalid sized room.
        //

        let mut socket = create_socket!(socket_addr);

        write_message!(socket, ClientPacket::Create { size: Some(0) });
        read_message!(socket, ServerPacket::Error { message } => assert_eq!("The room size is not valid", message));

        write_message!(socket, ClientPacket::Create { size: Some(255) });
        read_message!(socket, ServerPacket::Error { message } => assert_eq!("The room size is not valid", message));

        //
        // Test creating a valid room.
        //  

        write_message!(socket, ClientPacket::Create { size: None });

        let room_id = read_message!(socket, ServerPacket::Create { id } => id);


        //
        // Test joining an non-existent room.
        //
        
        let mut socket_2 = create_socket!(socket_addr);
        
        write_message!(socket_2, ClientPacket::Join { id: String::new() });
        read_message!(socket_2, ServerPacket::Error { message } => assert_eq!("The room does not exist.", message));

        //
        // Test joining the room.
        //

        write_message!(socket_2, ClientPacket::Join { id: room_id.clone() });

        read_message!(socket_2, ServerPacket::Join { size } => assert_eq!(Some(1), size));
        read_message!(socket, ServerPacket::Join { size } => assert_eq!(None, size));

        //
        // Test joining a full room.
        //

        let mut socket_3 = create_socket!(socket_addr);
        
        write_message!(socket_3, ClientPacket::Join { id: room_id.clone() });
        read_message!(socket_3, ServerPacket::Error { message } => assert_eq!("The room is full.", message));

        //
        // Test joining a removed room.
        //
        
        close_socket!(socket);
        close_socket!(socket_2);

        write_message!(socket_3, ClientPacket::Join { id: room_id.clone() });
        read_message!(socket_3, ServerPacket::Error { message } => assert_eq!("The room does not exist.", message));

        //
        // Test creating a single-occupant room.
        // 

        write_message!(socket_3, ClientPacket::Create { size: Some(1) });

        let room_id = read_message!(socket_3, ServerPacket::Create { id } => id);

        //
        // Test joining a single-occupant room.
        //

        let mut socket_4 = create_socket!(socket_addr);

        write_message!(socket_4, ClientPacket::Join { id: room_id });
        read_message!(socket_4, ServerPacket::Error { message } => assert_eq!("The room is full.", message));

        close_socket!(socket_3);
        close_socket!(socket_4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn communication() 
    {
        //
        // The maximum number of clients to test.
        //

        const N: u8 = u8::MAX - 1;

        //
        // Setup test.
        //

        let socket_addr = setup().await;

        //
        // Create N clients, the first client creates a room, the rest join the room.
        //

        let mut sockets = vec![];
        let mut room_id = String::new();

        for expected_size in 0..usize::from(N)
        {
            let mut socket = create_socket!(socket_addr);

            if expected_size == 0 
            {
                write_message!(socket, ClientPacket::Create { size: Some(N.into()) });
                read_message!(socket, ServerPacket::Create { id } => room_id = id);

                sockets.push(socket);
            }
            else 
            {
                write_message!(socket, ClientPacket::Join { id: room_id.clone() } );
                sockets.push(socket);

                let size = sockets.len() - 1;
                for (index, socket) in sockets.iter_mut().enumerate()
                {
                    if index == size
                    {
                        read_message!(socket, ServerPacket::Join { size } => assert_eq!(Some(expected_size), size));
                    }
                    else 
                    {
                        read_message!(socket, ServerPacket::Join { size } => assert_eq!(None, size));
                    }
                }
            }
        }   
        
        //
        // Test room bounds.
        //

        let mut socket = create_socket!(socket_addr);

        write_message!(socket, ClientPacket::Join { id: room_id.clone() } );
        read_message!(socket, ServerPacket::Error { message } => assert_eq!("The room is full.", message));  

        close_socket!(socket);

        //
        // Test broadcasting.
        //

        for expected_source in 0..N
        {
            let source_socket = &mut sockets[usize::from(expected_source)];

            write_binary_message!(source_socket, vec![ u8::MAX ]);
            write_binary_message!(source_socket, vec![ u8::MAX, 0, 1, 2, 3 ]);

            for expected_destination in 0..N
            {
                if expected_destination == expected_source 
                {
                    continue;
                }

                let destination_socket = &mut sockets[usize::from(expected_destination)];

                assert_eq!(vec![ expected_source ], read_binary_message!(destination_socket));
                assert_eq!(vec![ expected_source, 0, 1, 2, 3 ], read_binary_message!(destination_socket));
            } 
        }  

        //
        // Test sending to each other.
        //

        for expected_source in 0..N
        {
            for expected_destination in 0..N
            {
                let source_socket = &mut sockets[usize::from(expected_source)];

                write_binary_message!(source_socket, vec![ expected_destination ]);
                write_binary_message!(source_socket, vec![ expected_destination, 0, 1, 2, 3 ]);

                let destination_socket = &mut sockets[usize::from(expected_destination)];

                assert_eq!(vec![ expected_source ], read_binary_message!(destination_socket));
                assert_eq!(vec![ expected_source, 0, 1, 2, 3 ], read_binary_message!(destination_socket));
            } 
        }  

        //
        // Close host and N clients.
        //
        
        for _ in 0..N
        {
            let mut socket = sockets.remove(0);
            close_socket!(socket);

            for socket in &mut sockets
            {
                read_message!(socket, ServerPacket::Leave { index } => assert_eq!(0, index));
            }
        }

        //
        // Test if room was removed.
        //

        let mut socket = create_socket!(socket_addr);

        write_message!(socket, ClientPacket::Join { id: room_id.clone() } );
        read_message!(socket, ServerPacket::Error { message } => assert_eq!("The room does not exist.", message));  

        close_socket!(socket);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn indices() 
    { 
        //
        // The maximum number of clients to test.
        //

        const N: u8 = u8::MAX - 1;

        //
        // Setup test.
        //

        let socket_addr = setup().await;

        //
        // Perform the test by either closing the connection or leaving the room.
        //

        for method in ["close", "leave"] 
        {
            for direction in ["first", "last"] 
            {
                //
                // Create N clients, the first client creates a room, the rest join the room.
                //

                let mut sockets = vec![];
                let mut room_id = String::new();

                for expected_size in 0..usize::from(N)
                {
                    let mut socket = create_socket!(socket_addr);

                    if expected_size == 0 
                    {
                        write_message!(socket, ClientPacket::Create { size: Some(N.into()) });
                        read_message!(socket, ServerPacket::Create { id } => room_id = id);

                        sockets.push(socket);
                    }
                    else 
                    {
                        write_message!(socket, ClientPacket::Join { id: room_id.clone() } );
                        sockets.push(socket);

                        let size = sockets.len() - 1;
                        for (index, socket) in sockets.iter_mut().enumerate()
                        {
                            if index == size
                            {
                                read_message!(socket, ServerPacket::Join { size } => assert_eq!(Some(expected_size), size));
                            }
                            else 
                            {
                                read_message!(socket, ServerPacket::Join { size } => assert_eq!(None, size));
                            }
                        }
                    }
                } 

                //
                // Test leave room indices.
                //

                for mut expected_index in (0..usize::from(N)).rev()
                {
                    let mut socket = if direction == "last" { 
                        sockets.pop().unwrap()
                    } else {
                        sockets.remove(0)
                    };

                    if direction == "first" 
                    {
                        expected_index = 0;
                    }

                    if method == "leave"
                    {
                        write_message!(socket, ClientPacket::Leave);
                    }
                    else if method == "close"
                    {
                        close_socket!(socket);
                    }

                    for socket in &mut sockets
                    {
                        read_message!(socket, ServerPacket::Leave { index } => assert_eq!(expected_index, index));
                    }

                    if method == "leave"
                    {
                        close_socket!(socket);
                    }
                }
            } 
        }         
    }
}