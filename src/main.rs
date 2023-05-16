use ws::{listen};

mod relay;

fn main() 
{
    let relay = relay::Relay::new();

    listen("127.0.0.1:8000", 
 |sender| relay::Client::new(relay.clone(), sender)
    ).expect("Failed to listen");
}