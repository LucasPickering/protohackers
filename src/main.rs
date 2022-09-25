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
use std::sync::Arc;
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
            2 => Ok(Box::new(PriceTrackingServer)),
            problem => Err(anyhow!("Unknown problem: {}", problem).into()),
        }
    }
}

/// An implementation of a Protohackers server. There will be one implementation
/// per problem. It's important that is both `Send` and `Sync`, because a single
/// instance of this trait is going to be instantiated per _program_ session.
/// That means each client that connects is going to run on the same server
/// instance. If you need mutability within your server, you'll need to
/// implement internal mutability within the server. This is necessary because
/// there could be multiple clients connected simultaneously.
#[async_trait]
trait ProtoServer: Send + Sync {
    async fn handle_client(&self, socket: TcpStream) -> ServerResult<()>;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .target(Target::Stdout)
        .init();
    let args = Args::parse();
    // This needs an Arc so we can pass 'static copies into each handler task
    let server = Arc::new(args.get_server()?);
    let listener = TcpListener::bind((args.host.as_str(), args.port)).await?;
    info!("Listening on {}:{}", args.host, args.port);

    loop {
        let (socket, client) = listener.accept().await?;

        info!("{} Connected", client);

        let server = Arc::clone(&server);
        tokio::spawn(async move {
            match server.handle_client(socket).await {
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
