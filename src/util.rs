use crate::error::{ServerError, ServerResult};
use anyhow::Context;
use log::debug;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Read a message from a socket, and log the message
pub async fn socket_read(
    mut reader: impl AsyncReadExt + Unpin,
    buffer: &mut [u8],
) -> ServerResult<&[u8]> {
    let bytes_read = reader
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
    mut writer: impl AsyncWriteExt + Unpin,
    bytes: &[u8],
) -> ServerResult<()> {
    debug!("=> {:?}", String::from_utf8_lossy(bytes));
    Ok(writer
        .write_all(bytes)
        .await
        .context("Error writing to socket")?)
}

/// Extensions on the `Result` type. This is wildly unnecessary but it was fun
/// to write.
pub trait ResultExt<T> {
    /// "Smush" a result together by extracting either the `Ok` or `Err` value,
    /// whichever is present. Requires the `Ok` and `Err` variants to be of the
    /// same generic type, i.e. a `Result<T, T>`.
    fn smush(self) -> T;
}

impl<T> ResultExt<T> for Result<T, T> {
    fn smush(self) -> T {
        match self {
            Ok(val) => val,
            Err(val) => val,
        }
    }
}
