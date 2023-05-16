use ws::{Builder, Settings};

mod relay;

fn main() 
{
    let relay = relay::Relay::new();
    let ws = Builder::new()
        .with_settings(Settings {
            queue_size: 10000,
            ..Settings::default()
        })
        .build(|sender| relay::Client::new(relay.clone(), sender))
        .expect("Failed to build WebSocket server.");

    ws.listen("0.0.0.0:1234").expect("Failed to start WebSocket server.");
}