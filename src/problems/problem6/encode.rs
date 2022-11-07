//! Encoding data into the Speed Daemon binary format

use crate::problems::problem6::{
    Observation, ServerMessage, Ticket, Timestamp,
};

impl ServerMessage {
    /// Encode a message into the Speed Daemon binary format
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.encode_into(&mut bytes);
        bytes
    }

    /// Get the tag byte that starts off this message
    fn tag(&self) -> u8 {
        match self {
            ServerMessage::Error { .. } => 0x10,
            ServerMessage::Ticket { .. } => 0x21,
            ServerMessage::Heartbeat => 0x41,
        }
    }
}

/// Anything that can be encoded into the Speed Daemon binary format
pub trait Encode {
    /// Encode this data into the given buffer
    fn encode_into(&self, bytes: &mut Vec<u8>);
}

impl Encode for u8 {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        bytes.push(*self);
    }
}

impl Encode for &[u8] {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        bytes.extend(*self);
    }
}

impl Encode for ServerMessage {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        // Add the tag byte first
        self.tag().encode_into(bytes);
        // Add the message body
        match self {
            Self::Error { message } => {
                message.encode_into(bytes);
            }
            Self::Ticket {
                ticket:
                    Ticket {
                        plate,
                        road,
                        observation1,
                        observation2,
                        speed,
                    },
            } => {
                plate.encode_into(bytes);
                road.encode_into(bytes);
                observation1.encode_into(bytes);
                observation2.encode_into(bytes);
                speed.encode_into(bytes);
            }
            Self::Heartbeat => {}
        }
    }
}

impl Encode for Observation {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        self.mile.encode_into(bytes);
        self.timestamp.encode_into(bytes);
    }
}

impl Encode for String {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        // String format is defined in the problem statement
        // Length must be a **byte**
        (self.len() as u8).encode_into(bytes);
        self.as_bytes().encode_into(bytes);
    }
}

impl Encode for u16 {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        self.to_be_bytes().as_slice().encode_into(bytes);
    }
}

impl Encode for u32 {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        self.to_be_bytes().as_slice().encode_into(bytes);
    }
}

impl Encode for Timestamp {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        self.0.encode_into(bytes);
    }
}

impl<T: Encode> Encode for &T {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        (*self).encode_into(bytes);
    }
}
