//! Decoding data from the Speed Daemon binary format

use crate::{
    error::ServerResult,
    problems::problem6::{
        Camera, ClientMessage, Mile, Road, SpeedLimit, Timestamp,
    },
};
use anyhow::{anyhow, Context};
use async_stream::try_stream;
use bytes::BytesMut;
use futures::Stream;
use log::{debug, info};
use nom::{
    branch::alt,
    bytes::streaming::tag,
    combinator::map,
    multi::count,
    number::{
        streaming::{u16, u32, u8},
        Endianness,
    },
    sequence::{preceded, tuple},
    IResult,
};
use tokio::{io::AsyncReadExt, net::tcp::OwnedReadHalf};

const ENDIANNESS: Endianness = Endianness::Big;
type Input<'a> = &'a [u8];

#[derive(Debug)]
pub struct Decoder {
    reader: OwnedReadHalf,
    buffer: BytesMut,
}

impl Decoder {
    pub fn new(reader: OwnedReadHalf) -> Self {
        Self {
            reader,
            buffer: BytesMut::with_capacity(256),
        }
    }

    /// Parse as many messages as possible from the socket. This will keep
    /// reading bytes and parsing them until we can't find a valid message in
    /// there anymore. This allows us to parse packets that hold multiple
    /// messages.
    pub(super) fn read_messages(
        &mut self,
    ) -> impl '_ + Stream<Item = ServerResult<ParseOutcome>> {
        try_stream! {
            loop {
                // Load bytes into the buffer
                let bytes_read = self
                    .reader
                    .read_buf(&mut self.buffer)
                    .await
                    .context("Error reading TCP buffer")?;

                // TODO should we look for EOF instead?
                if bytes_read == 0 {
                    break;
                }

                while !self.buffer.is_empty() {
                    match self.decode_next_message() {
                        Some(outcome) => yield outcome,
                        // Go back to the outer loop so we can wait for more data
                        None => break,
                    }
                }
            }
        }
    }

    fn decode_next_message(&mut self) -> Option<ParseOutcome> {
        let peer_addr = self.reader.peer_addr().unwrap();
        match parse_message(&self.buffer) {
            Ok((remaining, message)) => {
                let consumed = self.buffer.len() - remaining.len();
                info!(
                    "{} => {:?} (from {:02x?})",
                    peer_addr,
                    message,
                    &self.buffer[..consumed]
                );
                self.consume_bytes(consumed);
                Some(ParseOutcome::Success(message))
            }
            // We didn't have enough bytes to form a full message. Just
            // return nothing and hope we get more data next
            // time
            Err(nom::Err::Incomplete(needed)) => {
                debug!(
                    "{} Incomplete message (needed {:?}): {:02x?}",
                    peer_addr, needed, self.buffer
                );
                None
            }
            // This is a hack to check for an incorrect tag byte. This
            // parsing error should be recoverable, but all others are
            // too far gone
            Err(nom::Err::Failure(nom::error::Error {
                code: nom::error::ErrorKind::Tag,
                input,
            })) => {
                // TODO is this the correct number of bytes to eat? I *think*
                // input is everything up to where the error occurred
                self.consume_bytes(input.len());
                Some(ParseOutcome::IllegalMessageType)
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Some(ParseOutcome::UnknownError(anyhow!(
                    "Malformed message from client {}: {:02x?} {:?}",
                    peer_addr,
                    self.buffer,
                    e
                )))
            }
        }
    }

    /// Remove the first `n` bytes from the buffer
    fn consume_bytes(&mut self, n: usize) {
        let _ = self.buffer.split_to(n);
    }
}

pub(super) enum ParseOutcome {
    Success(ClientMessage),
    IllegalMessageType,
    UnknownError(anyhow::Error),
}

/// Decode a message from binary data. This supports incomplete or
/// over-complete parsing, meaning there can be an incomplete message or
/// additional bytes beyond the message ending in the buffer. This allows
/// for streamed parsing from a socket.
fn parse_message(bytes: Input) -> IResult<Input, ClientMessage> {
    alt((
        // Plate
        preceded(
            tag(&[0x20]),
            map(tuple((string, timestamp)), |(plate, timestamp)| {
                ClientMessage::Plate { plate, timestamp }
            }),
        ),
        // WantHeartbeat
        preceded(
            tag(&[0x40]),
            map(u32(ENDIANNESS), |interval| ClientMessage::WantHeartbeat {
                interval,
            }),
        ),
        // IAmCamera
        preceded(
            tag(&[0x80]),
            map(
                tuple((road, mile, speed_limit)),
                |(road, mile, speed_limit)| ClientMessage::IAmCamera {
                    camera: Camera {
                        road,
                        mile,
                        speed_limit,
                    },
                },
            ),
        ),
        // IAmDispatch
        preceded(
            tag(&[0x81]),
            map(len_prefixed(u16(ENDIANNESS)), |roads| {
                ClientMessage::IAmDispatcher { roads }
            }),
        ),
    ))(bytes)
}

/// Parse a length-prefixed array of elements. The length is the number of
/// *elements*, not bytes, to parse. The prefix is a byte, meaning max length
/// is 255 elements.
fn len_prefixed<'a, T>(
    element: impl Fn(Input<'a>) -> IResult<Input<'a>, T>,
) -> impl Fn(Input<'a>) -> IResult<Input<'a>, Vec<T>> {
    move |input| {
        let (input, len) = u8(input)?;
        count(&element, len.into())(input)
    }
}

/// Parse a length-prefixed UTF-8 string.
fn string(input: Input) -> IResult<Input, String> {
    let (input, bytes) = len_prefixed(u8)(input)?;
    // We assume the string is valid UTF-8, cause that seems reasonable
    Ok((input, String::from_utf8_lossy(&bytes).into()))
}

/// Parse a u32 timestamp
fn timestamp(input: Input) -> IResult<Input, Timestamp> {
    map(u32(ENDIANNESS), Timestamp)(input)
}

/// Parse a u16 road number
fn road(input: Input) -> IResult<Input, Road> {
    u16(ENDIANNESS)(input)
}

/// Parse a u16 mile number
fn mile(input: Input) -> IResult<Input, Mile> {
    u16(ENDIANNESS)(input)
}

/// Parse a u16 speed limit (MPH)
fn speed_limit(input: Input) -> IResult<Input, SpeedLimit> {
    u16(ENDIANNESS)(input)
}
