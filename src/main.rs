mod problem0;
mod problem1;

use crate::{problem0::EchoServer, problem1::PrimeTestServer};
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use clap::Parser;
use env_logger::{Env, Target};
use log::{debug, error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// TCP server for Protohackers
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Number of the problem whose server should be executed
    #[clap(value_parser)]
    problem: u8,

    /// IP/hostname to bind to
    #[clap(long, value_parser, default_value = "127.0.0.1")]
    host: String,

    /// Port number to host on
    #[clap(short, long, value_parser, default_value_t = 8000)]
    port: u16,
}

impl Args {
    fn get_server(&self) -> anyhow::Result<Box<dyn ProtoServer>> {
        match self.problem {
            0 => Ok(Box::new(EchoServer)),
            1 => Ok(Box::new(PrimeTestServer)),
            problem => Err(anyhow!("Unknown problem: {}", problem)),
        }
    }
}

#[async_trait]
trait ProtoServer: Send + Sync {
    async fn run_server(&self, socket: TcpStream) -> anyhow::Result<()>;

    async fn read<'a>(
        &self,
        socket: &mut TcpStream,
        buffer: &'a mut [u8],
    ) -> anyhow::Result<&'a [u8]> {
        let bytes_read = socket
            .read(buffer)
            .await
            .context("Error reading from socket")?;
        if bytes_read == 0 {
            Err(anyhow!("Socket closed"))
        } else {
            let received = &buffer[0..bytes_read];
            debug!(
                "{} Receive: {:?}",
                socket.peer_addr()?,
                String::from_utf8_lossy(received)
            );
            Ok(received)
        }
    }

    async fn write(
        &self,
        socket: &mut TcpStream,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        debug!(
            "{} Send: {:?}",
            socket.peer_addr()?,
            String::from_utf8_lossy(bytes)
        );
        socket
            .write_all(bytes)
            .await
            .context("Error writing to socket")
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .target(Target::Stdout)
        .init();
    let args = Args::parse();
    let listener = TcpListener::bind((args.host.as_str(), args.port)).await?;
    info!("Listening on {}:{}", args.host, args.port);

    loop {
        let (socket, client) = listener.accept().await?;
        // We need create a new logical handler for each socket, based on the
        // problem input argument from the user. This should be super cheap.
        let server = args.get_server()?;
        info!("{} Connected", client);

        tokio::spawn(async move {
            if let Err(e) = server.run_server(socket).await {
                if e.to_string() != "Socket closed" {
                    error!("{} Error running server: {}", client, e);
                }
            }
            info!("{} Disconnected", client);
        });
    }
}
