use std::sync::Arc;
use tokio::net::TcpListener;

mod relay;

#[tokio::main]
async fn main() {
    let relay = Arc::new(relay::Relay::new());

    let addr = "127.0.0.1:9002";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");

    println!("Listening on: {}", addr);

    while let Ok((tcp_stream, socket_addr)) = listener.accept().await 
    {
        let cloned_relay = relay.clone();

        tokio::spawn(async move {
            cloned_relay.handle_connection(tcp_stream, socket_addr).await;
        });
    }
}