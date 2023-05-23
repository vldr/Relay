use ws::{Builder, Settings};
use std::{env};

mod relay;
mod tests;

fn main() 
{
    let address = env::args().nth(1).unwrap_or("0.0.0.0".to_string());
    let port = env::args().nth(2).unwrap_or("1234".to_string());

    let server = relay::Server::new();
    let ws = Builder::new()
        .with_settings(Settings {
            max_connections: 10000,
            tcp_nodelay: true,
            ..Settings::default()
        })
        .build(|sender| relay::Client::new(&server, sender))
        .expect("Failed to build WebSocket server.");

    ws.listen(format!("{}:{}", address, port))
      .expect("Failed to start WebSocket server.");
}