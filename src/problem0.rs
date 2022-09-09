use crate::ProtoServer;
use async_trait::async_trait;
use log::error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Copy, Clone, Debug)]
pub struct EchoServer;

#[async_trait]
impl ProtoServer for EchoServer {
    async fn run_server(self, mut socket: TcpStream) {
        let mut buf = [0; 1024];
        // I'm not sure how this can fail... let's assume it won't
        let peer_addr = socket.peer_addr().unwrap();

        // In a loop, read data from the socket and write the data back.
        loop {
            let bytes_read = match socket.read(&mut buf).await {
                // socket closed
                Ok(n) if n == 0 => break,
                Ok(n) => n,
                Err(e) => {
                    error!("{} Error reading from socket: {}", peer_addr, e);
                    break;
                }
            };

            // Write the data back
            if let Err(e) = socket.write_all(&buf[0..bytes_read]).await {
                error!("{} Error writing to socket: {}", peer_addr, e);
                break;
            }
        }
    }
}
