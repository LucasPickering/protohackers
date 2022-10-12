use std::{io, string::FromUtf8Error};
use thiserror::Error;

pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Socket closed")]
    SocketClose,

    // These errors are really common so it's worth having variants, so we
    // don't need to convert to anyhow every time
    #[error(transparent)]
    Io(io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<io::Error> for ServerError {
    fn from(other: io::Error) -> Self {
        match other.kind() {
            io::ErrorKind::UnexpectedEof => ServerError::SocketClose,
            _ => ServerError::Io(other),
        }
    }
}
