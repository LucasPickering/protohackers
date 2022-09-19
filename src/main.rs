mod error;
mod problem0;
mod problem1;
mod problem2;
mod util;

use crate::{
    error::{ServerError, ServerResult},
    problem0::EchoServer,
    problem1::PrimeTestServer,
    problem2::PriceTrackingServer,
};
use anyhow::anyhow;
use async_trait::async_trait;
use clap::Parser;
use env_logger::{Env, Target};
use log::{error, info};
use tokio::net::{TcpListener, TcpStream};

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
    fn get_server(&self) -> ServerResult<Box<dyn ProtoServer>> {
        match self.problem {
            0 => Ok(Box::new(EchoServer)),
            1 => Ok(Box::new(PrimeTestServer)),
            2 => Ok(Box::new(PriceTrackingServer::new()?)),
            problem => Err(anyhow!("Unknown problem: {}", problem).into()),
        }
    }
}

#[async_trait]
trait ProtoServer: Send {
    async fn run_server(&mut self, socket: TcpStream) -> ServerResult<()>;
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
        // problem input argument from the user. This should only allocate
        // what's needed on a per-session basis.
        let mut server = args.get_server()?;
        info!("{} Connected", client);

        tokio::spawn(async move {
            match server.run_server(socket).await {
                // Ignore SocketClose because it's a normal error
                Ok(()) | Err(ServerError::SocketClose) => {}
                Err(error) => {
                    error!("{} Error running server: {:?}", client, error);
                }
            }
            info!("{} Disconnected", client);
        });
    }
}
