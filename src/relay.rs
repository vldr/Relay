use tokio::net::{TcpStream};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{accept_async, tungstenite::{Result, Error}};

use std::net::SocketAddr;

pub struct Relay 
{
    
} 

impl Relay {
    pub fn new() -> Relay 
    {
        Relay{}
    }

    pub async fn accept(&self, peer: SocketAddr, stream: TcpStream) 
    {
        if let Err(e) = self.handle(peer, stream).await 
        {
            match e 
            {
                Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                err => println!("Error processing connection: {}", err),
            }
        }
    }

    async fn handle(&self, _peer: SocketAddr, stream: TcpStream) -> Result<()> 
    {
        let mut ws_stream = accept_async(stream).await?;
    
        while let Some(Ok(msg)) = ws_stream.next().await 
        {
            if msg.is_text() 
            {
                ws_stream.send(msg).await?;
            } 
            else if msg.is_binary() 
            {
                
            }
        }
    
        Ok(())
    }
    
}