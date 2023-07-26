use std::env;
use tokio::net::TcpListener;

mod relay;
mod tests;

#[tokio::main]
async fn main() {
    let address = env::args().nth(1).unwrap_or("0.0.0.0".to_string());
    let port = env::args().nth(2).unwrap_or("0".to_string());
    let host = env::args().nth(3).unwrap_or("".to_string());

    let server = relay::Server::new();

    if let Ok(listener) = TcpListener::bind(&format!("{}:{}", address, port)).await {
        println!("Listening on: {}", listener.local_addr().unwrap());

        while let Ok((tcp_stream, _)) = listener.accept().await {
            tcp_stream.set_nodelay(true).unwrap();

            tokio::spawn(relay::Server::handle_connection(
                tcp_stream,
                server.clone(),
                host.clone(),
            ));
        }
    } else {
        println!("Failed to listen on: {}:{}", address, port);
    }
}
