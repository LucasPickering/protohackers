use crate::{
    error::{ServerError, ServerResult},
    problems::TcpServer,
    util::socket_write,
};
use anyhow::anyhow;
use async_trait::async_trait;
use derive_more::Display;
use fancy_regex::Regex;
use lazy_static::lazy_static;
use log::{error, info};
use std::{borrow::Cow, net::SocketAddr};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{
        self,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpSocket, TcpStream,
    },
};

const BIG_TONYS_FAT_SICILIAN_WALLET: &str = "7YWHMfk9JZe0LM0g1ZauHuiSxhI";
const UPSTREAM_CHAT_SERVER: (&str, u16) = ("chat.protohackers.com", 16963);

/// https://protohackers.com/problem/5
#[derive(Debug)]
pub struct MaliciousChatProxy;

#[async_trait]
impl TcpServer for MaliciousChatProxy {
    async fn handle_client(
        &self,
        client_stream: TcpStream,
    ) -> ServerResult<()> {
        // Split into reader/writer so we can do line buffering
        let (client_reader, client_writer) = client_stream.into_split();

        // First, open a connection with the real chat server
        let upstream_host = net::lookup_host(UPSTREAM_CHAT_SERVER)
            .await?
            .next()
            .ok_or_else(|| {
                anyhow!(
                    "Failed to resolve chat server by hostname {:?}",
                    UPSTREAM_CHAT_SERVER
                )
            })?;
        let (upstream_reader, upstream_writer) = TcpSocket::new_v4()?
            .connect(upstream_host)
            .await?
            .into_split();

        let mut upstream = ProxyStream::new(
            Direction::Upstream,
            client_reader,
            upstream_writer,
        )?;
        let mut downstream = ProxyStream::new(
            Direction::Downstream,
            upstream_reader,
            client_writer,
        )?;

        // Server => client runs in a separate task
        tokio::spawn(async move {
            loop {
                match downstream.proxy_message().await {
                    // Ignore SocketClose because it's a normal error
                    Ok(()) | Err(ServerError::SocketClose) => {}
                    Err(error) => {
                        error!("Error proxying downstream: {:?}", error);
                    }
                }
            }
        });

        // Client => server runs in the main task
        // If this loop breaks (e.g. from socket close), we'll drop the task
        // above and it'll exit
        loop {
            upstream.proxy_message().await?;
        }
    }
}

#[derive(Copy, Clone, Debug, Display)]
enum Direction {
    /// Client => server
    #[display(fmt = "=>")]
    Upstream,
    /// Server => client
    #[display(fmt = "<=")]
    Downstream,
}

struct ProxyStream {
    client_addr: SocketAddr,
    direction: Direction,
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    buffer: String,
}

impl ProxyStream {
    fn new(
        direction: Direction,
        reader: OwnedReadHalf,
        writer: OwnedWriteHalf,
    ) -> ServerResult<Self> {
        Ok(Self {
            // Client address is used for logging, figure out why one that is
            // now
            client_addr: match direction {
                Direction::Upstream => reader.peer_addr()?,
                Direction::Downstream => writer.peer_addr()?,
            },
            direction,
            reader: BufReader::new(reader),
            writer,
            buffer: String::new(),
        })
    }

    async fn proxy_message(&mut self) -> ServerResult<()> {
        let bytes_read = self.reader.read_line(&mut self.buffer).await?;

        if bytes_read > 0 {
            info!(
                "{} {} {:?} ({} bytes)",
                self.client_addr, self.direction, self.buffer, bytes_read
            );
            socket_write(
                &mut self.writer,
                rewrite_boguscoin(&self.buffer).as_bytes(),
            )
            .await?;
            self.buffer.clear();
            Ok(())
        } else {
            Err(ServerError::SocketClose)
        }
    }
}

fn rewrite_boguscoin(message: &str) -> Cow<str> {
    lazy_static! {
        static ref BOGUSCOIN_RE: Regex =
        // Message may end in a newline, we want to treat that as an end-of-string
            Regex::new("(^|(?<= ))7[A-Za-z0-9]{25,34}((?=[ \n])|$)").unwrap();
    }
    BOGUSCOIN_RE.replace_all(message, BIG_TONYS_FAT_SICILIAN_WALLET)
}
