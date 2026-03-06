//! Binary frame format of the z8run protocol.
//!
//! Structure:
//! | Offset | Size     | Field          | Description                    |
//! |--------|----------|----------------|--------------------------------|
//! | 0      | 1 byte   | version        | Protocol version               |
//! | 1      | 2 bytes  | msg_type       | Message type (enum)            |
//! | 3      | 4 bytes  | correlation_id | ID for request/response        |
//! | 7      | 4 bytes  | payload_len    | Payload length in bytes        |
//! | 11     | variable | payload        | Serialized data (bincode)      |

use thiserror::Error;

/// Current protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

/// Fixed header size in bytes.
pub const HEADER_SIZE: usize = 11;

/// Maximum allowed payload size (16 MB).
pub const MAX_PAYLOAD_SIZE: u32 = 16 * 1024 * 1024;

/// Protocol errors.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Unsupported protocol version: {0} (expected: {PROTOCOL_VERSION})")]
    UnsupportedVersion(u8),

    #[error("Unknown message type: {0}")]
    UnknownMessageType(u16),

    #[error("Payload exceeds maximum size: {size} bytes (maximum: {MAX_PAYLOAD_SIZE})")]
    PayloadTooLarge { size: u32 },

    #[error("Incomplete frame: expected {expected} bytes, received {received}")]
    IncompleteFrame { expected: usize, received: usize },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),
}

/// Fixed frame header.
#[derive(Debug, Clone, Copy)]
pub struct FrameHeader {
    pub version: u8,
    pub msg_type: u16,
    pub correlation_id: u32,
    pub payload_len: u32,
}

impl FrameHeader {
    /// Serializes the header to bytes (11 fixed bytes).
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0] = self.version;
        buf[1..3].copy_from_slice(&self.msg_type.to_le_bytes());
        buf[3..7].copy_from_slice(&self.correlation_id.to_le_bytes());
        buf[7..11].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }

    /// Deserializes a header from bytes.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, ProtocolError> {
        if buf.len() < HEADER_SIZE {
            return Err(ProtocolError::IncompleteFrame {
                expected: HEADER_SIZE,
                received: buf.len(),
            });
        }

        let version = buf[0];
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }

        let msg_type = u16::from_le_bytes([buf[1], buf[2]]);
        let correlation_id = u32::from_le_bytes([buf[3], buf[4], buf[5], buf[6]]);
        let payload_len = u32::from_le_bytes([buf[7], buf[8], buf[9], buf[10]]);

        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge { size: payload_len });
        }

        Ok(Self {
            version,
            msg_type,
            correlation_id,
            payload_len,
        })
    }
}

/// Complete frame: header + payload.
#[derive(Debug, Clone)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Creates a frame from a message type and serialized payload.
    pub fn new(
        msg_type: u16,
        correlation_id: u32,
        payload: Vec<u8>,
    ) -> Result<Self, ProtocolError> {
        let payload_len = payload.len() as u32;
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge { size: payload_len });
        }

        Ok(Self {
            header: FrameHeader {
                version: PROTOCOL_VERSION,
                msg_type,
                correlation_id,
                payload_len,
            },
            payload,
        })
    }

    /// Serializes the complete frame to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&self.header.to_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserializes a complete frame from bytes.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, ProtocolError> {
        let header = FrameHeader::from_bytes(buf)?;
        let total_size = HEADER_SIZE + header.payload_len as usize;

        if buf.len() < total_size {
            return Err(ProtocolError::IncompleteFrame {
                expected: total_size,
                received: buf.len(),
            });
        }

        let payload = buf[HEADER_SIZE..total_size].to_vec();
        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_roundtrip() {
        let original = Frame::new(1, 42, vec![1, 2, 3, 4, 5]).unwrap();
        let bytes = original.to_bytes();
        let decoded = Frame::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.header.version, PROTOCOL_VERSION);
        assert_eq!(decoded.header.msg_type, 1);
        assert_eq!(decoded.header.correlation_id, 42);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_header_size() {
        let header = FrameHeader {
            version: 1,
            msg_type: 0,
            correlation_id: 0,
            payload_len: 0,
        };
        assert_eq!(header.to_bytes().len(), HEADER_SIZE);
    }

    #[test]
    fn test_version_check() {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[0] = 99; // invalid version
        assert!(FrameHeader::from_bytes(&bytes).is_err());
    }
}
