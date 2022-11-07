mod problem0;
mod problem1;
mod problem2;
mod problem3;
mod problem4;
mod problem5;
mod problem6;

use crate::{
    error::{ServerError, ServerResult},
    problems::{
        problem0::EchoServer, problem1::PrimeTestServer,
        problem2::PriceTrackingServer, problem3::ChatServer,
        problem4::UnusualDatabaseServer, problem5::MaliciousChatProxy,
        problem6::SpeedDaemonServer,
    },
};
use anyhow::anyhow;
use async_trait::async_trait;
use log::{error, info};
use std::{fmt::Debug, net::SocketAddr, sync::Arc};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

/// A solution for a single Protohackers problem. There are different
/// categories of servers, based on the type of traffic they handle. Each
/// category is then broken down into individual servers for each problem.
///
/// It's important that this is both `Send` and `Sync`, because a single
/// instance of this enum is going to be instantiated per _program_ session.
/// That means each client that connects is going to run on the same server
/// instance. If you need mutability within your server, you'll need to
/// implement internal mutability within the server. This is necessary because
/// there could be multiple clients connected simultaneously.

#[derive(Debug)]
pub enum ProtoServer {
    Tcp(Arc<dyn TcpServer>),
    Udp(Arc<dyn UdpServer>),
}

impl ProtoServer {
    /// Build a server for a given Protohackers problem
    pub fn new(problem_number: u8) -> ServerResult<Self> {
        match problem_number {
            0 => Ok(Self::Tcp(Arc::new(EchoServer))),
            1 => Ok(Self::Tcp(Arc::new(PrimeTestServer))),
            2 => Ok(Self::Tcp(Arc::new(PriceTrackingServer))),
            3 => Ok(Self::Tcp(Arc::new(ChatServer::new()))),
            4 => Ok(Self::Udp(Arc::new(UnusualDatabaseServer::new()))),
            5 => Ok(Self::Tcp(Arc::new(MaliciousChatProxy))),
            6 => Ok(Self::Tcp(Arc::new(SpeedDaemonServer::new()))),
            problem => Err(anyhow!("Unknown problem: {}", problem).into()),
        }
    }

    pub async fn run(self, host: &str, port: u16) -> ServerResult<()> {
        match self {
            Self::Tcp(server) => run_tcp(server, host, port).await,
            Self::Udp(server) => run_udp(server, host, port).await,
        }
    }
}

/// A solution server for a TCP-based problem. This will listen for new clients,
/// then each time one connects, it will be passed along to the server to
/// handle the business logic.
#[async_trait]
pub trait TcpServer: Debug + Send + Sync {
    /// Called when a client first connects. The server can freely send and
    /// receive data with the client.
    async fn handle_client(&self, socket: TcpStream) -> ServerResult<()>;
}

/// Run a TCP server. This will start a loop that listens for a new client,
/// then spawns an async task to handle that client.
async fn run_tcp(
    server: Arc<dyn TcpServer>,
    host: &str,
    port: u16,
) -> ServerResult<()> {
    let listener = TcpListener::bind((host, port)).await?;
    info!("Listening on {}:{} (TCP)", host, port);

    loop {
        let (socket, client) = listener.accept().await?;

        info!("{} Connected", client);

        // Clone the server reference so we can pass it into the task
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

/// A solution server for a UDP-based problem.
#[async_trait]
pub trait UdpServer: Debug + Send + Sync {
    /// Called whenever the server receives data from a client. The server can
    /// then freely handle the data and transmit back as necessary.
    async fn handle_data(
        &self,
        socket: &UdpSocket,
        data: &[u8],
        sender: &SocketAddr,
    ) -> ServerResult<()>;
}

/// Run a UDP server. This will listen for data from any client, then forward
/// it to the server to be handled. This does *not* listen for connections,
/// just messages. It is running in a [one-to-many format](https://docs.rs/tokio/1.21.2/tokio/net/struct.UdpSocket.html#example-one-to-many-bind).
async fn run_udp(
    server: Arc<dyn UdpServer>,
    host: &str,
    port: u16,
) -> ServerResult<()> {
    let socket = Arc::new(UdpSocket::bind((host, port)).await?);
    info!("Listening on {}:{} (UDP)", host, port);

    let mut buf = [0; 1024];
    loop {
        let (len, sender) = socket.recv_from(&mut buf).await?;
        info!("{} Received {} bytes", sender, len);

        // Clone the references so we can pass them into the task
        let socket = Arc::clone(&socket);
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            match server.handle_data(&socket, &buf[0..len], &sender).await {
                // Ignore SocketClose because it's a normal error
                Ok(()) | Err(ServerError::SocketClose) => {}
                Err(error) => {
                    error!("{} Error running server: {:?}", sender, error);
                }
            }
            info!("{} Disconnected", sender);
        });
    }
}
