mod error;
mod problems;
mod util;

use crate::problems::ProtoServer;
use clap::Parser;
use env_logger::{Env, Target};
use log::info;

/// Network server for solving Protohackers problems
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .target(Target::Stdout)
        .init();
    let args = Args::parse();

    let server = ProtoServer::new(args.problem)?;
    info!("Running problem #{}", args.problem);
    server.run(&args.host, args.port).await?;
    Ok(())
}
