use crate::error::{ServerError, ServerResult};
use anyhow::Context;
use log::debug;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Read a message from a socket, and log the message
pub async fn socket_read(
    mut socket: impl AsyncReadExt + Unpin,
    buffer: &mut [u8],
) -> ServerResult<&[u8]> {
    let bytes_read = socket
        .read(buffer)
        .await
        .context("Error reading from socket")?;
    if bytes_read == 0 {
        Err(ServerError::SocketClose)
    } else {
        let received = &buffer[0..bytes_read];
        debug!("<= {:?}", String::from_utf8_lossy(received));
        Ok(received)
    }
}

/// Write a message to a socket, and log it
pub async fn socket_write(
    mut socket: impl AsyncWriteExt + Unpin,
    bytes: &[u8],
) -> ServerResult<()> {
    debug!("=> {:?}", String::from_utf8_lossy(bytes));
    Ok(socket
        .write_all(bytes)
        .await
        .context("Error writing to socket")?)
}
