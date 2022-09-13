use crate::ProtoServer;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use tokio::net::TcpStream;

const IS_PRIME_METHOD: &str = "isPrime";

#[derive(Clone, Debug, Deserialize)]
struct InputMessage {
    method: String,
    number: i32,
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
    async fn run_server(&self, mut socket: TcpStream) -> anyhow::Result<()> {
        let mut buf = [0; 32000];

        loop {
            let input_bytes = self.read(&mut socket, &mut buf).await?;
            let reader = BufReader::new(input_bytes);
            let mut output_bytes = Vec::new();

            for line in reader.lines() {
                let line = line?;
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
                output_bytes.extend(serde_json::to_vec(&output_message)?);
                output_bytes.push(b'\n');
            }

            self.write(&mut socket, &output_bytes).await?;
        }
    }
}

fn is_prime(n: i32) -> bool {
    // Out-of-bounds check
    if n <= 1 {
        return false;
    }

    // Now we know it's positive, do a simple primality check
    let n = n as u32;
    let max_test_num = (n as f32).sqrt() as u32;
    for i in 2..=max_test_num {
        if n % i == 0 {
            return false;
        }
    }
    true
}
