use crate::{error::ServerResult, problems::UdpServer};
use async_trait::async_trait;
use log::debug;
use std::{collections::HashMap, net::SocketAddr};
use tokio::{net::UdpSocket, sync::RwLock};

/// https://protohackers.com/problem/4
#[derive(Debug)]

pub struct UnusualDatabaseServer {
    /// Values store that will be modified/read during operation. Lock needed
    /// for concurrency
    values: RwLock<HashMap<String, String>>,
}

impl UnusualDatabaseServer {
    pub fn new() -> Self {
        let mut values = HashMap::new();
        // This value should never be overwritten!
        values.insert("version".to_owned(), "big booty judge judy".to_owned());
        Self {
            values: RwLock::new(values),
        }
    }
}

#[async_trait]
impl UdpServer for UnusualDatabaseServer {
    async fn handle_data(
        &self,
        socket: &UdpSocket,
        data: &[u8],
        sender: &SocketAddr,
    ) -> ServerResult<()> {
        let data = String::from_utf8(data.into())?;

        // If the data has an `=` in it, then we split on that for key/value
        // Only split on the first `=`. Subsequent ones are just part of the
        // value
        match data.split_once('=') {
            // The `version` key is special and should never be overwritten
            Some((key, _)) if key == "version" => {}

            // Insert/update
            Some((key, value)) => {
                let mut values = self.values.write().await;
                values.insert(key.into(), value.into());
                debug!("{} Set `{}` = `{}`", sender, key, value);
            }

            // Retrieval
            None => {
                // The full message is the requested key
                let key = data;
                // Grab a read lock on the database. Use lexical scoping to
                // minimize the amount of time we have it open.
                let response = {
                    let values = self.values.read().await;
                    let value =
                        values.get(&key).map(String::as_str).unwrap_or("");
                    format!("{}={}", key, value)
                };
                debug!("{} Retrieve {}", sender, response);
                // Send the response back
                socket.send_to(response.as_bytes(), sender).await?;
            }
        }

        Ok(())
    }
}
