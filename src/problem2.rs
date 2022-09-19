use crate::{error::ServerResult, util::socket_write, ProtoServer};
use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, info};
use rusqlite::Connection;

use tokio::{
    io::{AsyncReadExt, BufReader},
    net::TcpStream,
};

#[derive(Debug)]
pub struct PriceTrackingServer {
    connection: Connection,
}

impl PriceTrackingServer {
    pub fn new() -> anyhow::Result<Self> {
        let connection = Connection::open_in_memory()?;
        connection.execute(
            "CREATE TABLE prices (
                id          INTEGER PRIMARY KEY,
                timestamp   INTEGER NOT NULL,
                price       INTEGER NOT NULL
            )",
            (),
        )?;
        Ok(Self { connection })
    }

    /// Query the mean price over a given time period. Start and end are both
    /// inclusive, i.e. a closed range. If the range has no samples, the mean
    /// is 0.
    fn query(
        &self,
        start_timestamp: i32,
        end_timestamp: i32,
    ) -> anyhow::Result<i32> {
        let mut statement = self.connection.prepare(
            "
            SELECT IFNULL(AVG(price), 0) FROM prices
            WHERE ?1 <= timestamp AND timestamp <= ?2
            ",
        )?;
        let mut rows = statement
            .query(rusqlite::params![start_timestamp, end_timestamp])?;

        // There should always be exactly one row, and the query defaults to
        // zero when the filter has no matches.
        let mean: f64 = rows.next()?.unwrap().get(0)?;
        Ok(mean.round() as i32)
    }

    /// Insert a new timestamp+price pairing into the data map
    fn insert(&mut self, timestamp: i32, price: i32) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO prices (timestamp, price) VALUES (?1, ?2)",
            (timestamp, price),
        )?;
        Ok(())
    }
}

#[async_trait]
impl ProtoServer for PriceTrackingServer {
    async fn run_server(&mut self, mut socket: TcpStream) -> ServerResult<()> {
        let (reader, mut writer) = socket.split();
        let mut reader = BufReader::new(reader);

        loop {
            // Read message according to the 9-byte format. First byte is the
            // action, next 4 are the first int32, last 4 are the second int32
            let action = reader.read_u8().await? as char;
            let i1 = reader.read_i32().await?;
            let i2 = reader.read_i32().await?;
            debug!("<= {} {} {}", action, i1, i2);

            // RUn either query or insert
            match action {
                'Q' => {
                    // Args are [start, end] of query timestamp range
                    let mean = self.query(i1, i2)?;
                    info!("Q [{}, {}] => {}", i1, i2, mean);
                    socket_write(&mut writer, &mean.to_be_bytes()).await?;
                }
                // Args are (timestamp, price)
                'I' => self.insert(i1, i2)?,
                c => return Err(anyhow!("Invalid action: '{}'", c).into()),
            };
        }
    }
}
