use std::sync::Arc;
use tokio::net::TcpListener;

mod relay;

#[tokio::main]
async fn main() {
    let relay = Arc::new(relay::Relay::new());

    let addr = "127.0.0.1:9002";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");

    println!("Listening on: {}", addr);

    while let Ok((stream, peer)) = listener.accept().await 
    {
        let relay_clone = relay.clone();

        tokio::spawn(async move {
            relay_clone.accept(peer, stream).await;
        });
    }

}