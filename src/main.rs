mod problem0;

use crate::problem0::EchoServer;
use anyhow::anyhow;
use async_trait::async_trait;
use clap::Parser;
use env_logger::{Env, Target};
use log::info;
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
    fn get_server(&self) -> anyhow::Result<impl ProtoServer> {
        match self.problem {
            0 => Ok(EchoServer),
            problem => Err(anyhow!("Unknown problem: {}", problem)),
        }
    }
}

#[async_trait]
trait ProtoServer: Copy {
    async fn run_server(self, socket: TcpStream);
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .target(Target::Stdout)
        .init();
    let args = Args::parse();
    let server = args.get_server()?; // Pick the problem
    let listener = TcpListener::bind((args.host.as_str(), args.port)).await?;
    info!("Listening on {}:{}", args.host, args.port);

    loop {
        let (socket, client) = listener.accept().await?;
        info!("{} Connected", client);

        tokio::spawn(async move {
            server.run_server(socket).await;
            info!("{} Disconnected", client);
        });
    }
}
