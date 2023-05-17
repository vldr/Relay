use ws::{Builder, Settings};

mod relay;
mod tests;

fn main() 
{
    let server = relay::Server::new();
    let ws = Builder::new()
        .with_settings(Settings {
            max_connections: 10000,
            ..Settings::default()
        })
        .build(|sender| relay::Client::new(server.clone(), sender))
        .expect("Failed to build WebSocket server.");

    ws.listen("0.0.0.0:1234").expect("Failed to start WebSocket server.");
}