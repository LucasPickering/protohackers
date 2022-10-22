use crate::{error::ServerResult, problems::TcpServer, util::socket_write};
use async_trait::async_trait;
use fancy_regex::Regex;
use lazy_static::lazy_static;
use log::debug;
use std::borrow::Cow;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{self, TcpSocket, TcpStream},
    select,
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
        let (client_reader, mut client_writer) = client_stream.into_split();
        let client_reader = BufReader::new(client_reader);
        let mut client_lines = client_reader.lines();

        // First, open a connection with the real chat server
        // TODO gracefully handle resolution failure
        let upstream_host = net::lookup_host(UPSTREAM_CHAT_SERVER)
            .await?
            .next()
            .unwrap();
        let (upstream_reader, mut upstream_writer) = TcpSocket::new_v4()?
            .connect(upstream_host)
            .await?
            .into_split();
        let upstream_reader = BufReader::new(upstream_reader);
        let mut upstream_lines = upstream_reader.lines();

        // Read from the upstream and downstream sockers, and forward messages
        // from one to the other.
        loop {
            // Poll both streams, and handle whichever one pops up first
            let (result, writer, direction) = select! {
                result = client_lines.next_line()  => {
                    (result, &mut upstream_writer, "=>")
                },
                result = upstream_lines.next_line() => {
                    (result, &mut client_writer, "<=")
                },
            };

            // Unpack the result and handle terminated stream. If one side is
            // closed, we want to close the other too by breaking the loop
            let mut message = match result? {
                Some(message) => message,
                None => break,
            };

            debug!("{} {}", direction, &message);
            message.push('\n'); // Newline needed to terminate the message
            let message = rewrite_boguscoin(&message); // teehee
            socket_write(writer, message.as_bytes()).await?;
        }

        Ok(())
    }
}

fn rewrite_boguscoin(message: &str) -> Cow<str> {
    lazy_static! {
        static ref BOGUSCOIN_RE: Regex =
            Regex::new("(^|(?<= ))7[A-Za-z0-9]{25,34}(?=\\s)").unwrap();
    }
    BOGUSCOIN_RE.replace_all(message, BIG_TONYS_FAT_SICILIAN_WALLET)
}
