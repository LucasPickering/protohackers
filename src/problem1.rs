use crate::{error::ServerResult, util::socket_write, ProtoServer};
use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpStream,
};

const IS_PRIME_METHOD: &str = "isPrime";

#[derive(Clone, Debug, Deserialize)]
struct InputMessage {
    method: String,
    /// Input numbers can be negative or decimal, even though those can't
    /// possibly be prime
    number: f64,
}

#[derive(Clone, Debug, Serialize)]
struct OutputMessage {
    method: String,
    prime: bool,
}

impl OutputMessage {
    fn new(prime: bool) -> Self {
        Self {
            method: IS_PRIME_METHOD.into(),
            prime,
        }
    }

    fn malformed() -> Self {
        Self {
            method: "MALFORMED".into(),
            prime: false,
        }
    }
}

/// Problem 1 - test if the input number is prime
#[derive(Copy, Clone, Debug)]
pub struct PrimeTestServer;

#[async_trait]
impl ProtoServer for PrimeTestServer {
    async fn run_server(&mut self, mut socket: TcpStream) -> ServerResult<()> {
        // Split the stream into buffered reader+writer so we can do
        // line-delimited operations
        let (reader, mut writer) = socket.split();
        let reader = BufReader::new(reader);
        let mut lines = reader.lines();

        // Read each line from the socket, process it, and send the response
        while let Some(line) = lines.next_line().await? {
            debug!("<= {:?}", line);
            let input_message_result =
                serde_json::from_str::<InputMessage>(&line);
            let output_message = match input_message_result {
                Ok(input_message)
                    if input_message.method != IS_PRIME_METHOD =>
                {
                    OutputMessage::malformed()
                }
                Ok(input_message) => {
                    OutputMessage::new(is_prime(input_message.number))
                }
                Err(_) => OutputMessage::malformed(),
            };

            // Write the output. Make sure to include the newline
            let mut output_bytes = serde_json::to_vec(&output_message)?;
            output_bytes.push(b'\n');
            socket_write(&mut writer, &output_bytes).await?;
        }

        Ok(())
    }
}

/// Check if an input number is prime. Any non-natural number is automatically
/// not prime.
fn is_prime(n_float: f64) -> bool {
    if n_float != n_float.round() {
        return false;
    }

    // Now we know n is an integer, so we can convert it
    let n_int = n_float as i64;

    // Check if it's positive by converting to u64
    let n_int = match u64::try_from(n_int) {
        Ok(n) => n,
        Err(_) => return false,
    };

    // 0 and 1 aren't prime
    if n_int <= 1 {
        return false;
    }

    // Now we know it's positive, do a simple primality check
    let max_test_num = n_float.sqrt() as u64;
    for i in 2..=max_test_num {
        if n_int % i == 0 {
            return false;
        }
    }
    true
}
