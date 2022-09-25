use crate::{
    error::ServerResult,
    util::{socket_read, socket_write},
    ProtoServer,
};
use async_trait::async_trait;
use tokio::net::TcpStream;

/// Problem 0 - echo back input
#[derive(Copy, Clone, Debug)]
pub struct EchoServer;

#[async_trait]
impl ProtoServer for EchoServer {
    async fn handle_client(&self, mut socket: TcpStream) -> ServerResult<()> {
        let mut buf = [0; 1024];

        // In a loop, read data from the socket and write the data back
        loop {
            let bytes = socket_read(&mut socket, &mut buf).await?;
            socket_write(&mut socket, bytes).await?;
        }
    }
}
