//! Content digest helpers — same wire shape as K0a (`sha256:<64 hex>`).

use sha2::{Digest, Sha256};

/// Canonical content digest for knowledge registration.
pub fn content_digest_sha256(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn digest_format() {
        let d = content_digest_sha256(b"hello");
        assert!(d.starts_with("sha256:"));
        assert_eq!(d.len(), "sha256:".len() + 64);
        assert!(
            d.as_bytes()[7..]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        );
    }
}
