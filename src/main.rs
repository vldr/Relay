use tokio::net::{TcpListener};
use std::{env};

mod relay;
mod tests;

#[tokio::main]
async fn main()
{
    let address = env::args().nth(1).unwrap_or("0.0.0.0".to_string());
    let port = env::args().nth(2).unwrap_or("1234".to_string());

    let listener = TcpListener::bind(&format!("{}:{}", address, port)).await
        .expect("Failed to bind");

    let server = relay::Server::new();

    println!("Listening on: {}:{}", address, port);

    while let Ok((tcp_stream, _)) = listener.accept().await 
    {
        tokio::spawn( relay::Server::handle_connection(server.clone(), tcp_stream));
    }
}