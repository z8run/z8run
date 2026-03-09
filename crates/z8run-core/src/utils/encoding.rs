//! Shared base64 encoding/decoding utilities.
//!
//! Replaces duplicated implementations in tts.rs and stt.rs.

use base64::{engine::general_purpose, Engine as _};

/// Encode bytes to base64 string using standard encoding.
pub fn base64_encode(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}

/// Decode base64 string to bytes.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    general_purpose::STANDARD
        .decode(s)
        .map_err(|e| format!("Invalid base64: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = b"Hello, World!";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.to_vec(), decoded);
    }

    #[test]
    fn test_encode() {
        assert_eq!(base64_encode(b"test"), "dGVzdA==");
        assert_eq!(base64_encode(b"Hello, world!"), "SGVsbG8sIHdvcmxkIQ==");
    }

    #[test]
    fn test_decode() {
        assert_eq!(base64_decode("dGVzdA==").unwrap(), b"test");
        assert_eq!(
            base64_decode("SGVsbG8sIHdvcmxkIQ==").unwrap(),
            b"Hello, world!"
        );
    }

    #[test]
    fn test_decode_invalid() {
        assert!(base64_decode("!!!invalid!!!").is_err());
    }
}
