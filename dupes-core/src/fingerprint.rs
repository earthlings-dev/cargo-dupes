use std::fmt;

use crate::node::NormalizedNode;

/// A fingerprint of a normalized AST node, wrapping a u64 hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint(u64);

impl Fingerprint {
    /// Compute a deterministic fingerprint from bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let digest = blake3::hash(bytes);
        let mut prefix = [0_u8; 8];
        prefix.copy_from_slice(&digest.as_bytes()[..8]);
        Self(u64::from_be_bytes(prefix))
    }

    /// Compute a deterministic fingerprint from a debug representation.
    #[must_use]
    fn from_debug(value: &impl std::fmt::Debug) -> Self {
        Self::from_bytes(format!("{value:?}").as_bytes())
    }

    /// Compute a fingerprint from a normalized node.
    #[must_use]
    pub fn from_node(node: &NormalizedNode) -> Self {
        Self::from_debug(node)
    }

    /// Compute a fingerprint from a signature + body pair.
    #[must_use]
    pub fn from_sig_and_body(sig: &NormalizedNode, body: &NormalizedNode) -> Self {
        Self::from_bytes(format!("{sig:?}\n{body:?}").as_bytes())
    }

    /// Compute a composite fingerprint from a set of fingerprints.
    /// Sorts by u64 value for order-independence, then hashes the sorted sequence.
    #[must_use]
    pub fn from_fingerprints(fps: &[Self]) -> Self {
        let mut sorted: Vec<u64> = fps.iter().map(|fp| fp.0).collect();
        sorted.sort_unstable();
        Self::from_bytes(format!("{sorted:?}").as_bytes())
    }

    /// Get the raw u64 value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Convert to hex string.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:016x}", self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Option<Self> {
        u64::from_str_radix(s, 16).ok().map(Fingerprint)
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let fp = Fingerprint(0xdead_beef_1234_5678);
        let hex = fp.to_hex();
        assert_eq!(hex, "deadbeef12345678");
        let fp2 = Fingerprint::from_hex(&hex).unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn display_format() {
        let fp = Fingerprint(0x0000_0000_0000_0042);
        assert_eq!(format!("{fp}"), "0000000000000042");
    }

    #[test]
    fn from_hex_invalid() {
        assert!(Fingerprint::from_hex("not_hex").is_none());
    }

    #[test]
    fn composite_fingerprint_order_independent() {
        let fp1 = Fingerprint(1);
        let fp2 = Fingerprint(2);
        let fp3 = Fingerprint(3);
        assert_eq!(
            Fingerprint::from_fingerprints(&[fp1, fp2, fp3]),
            Fingerprint::from_fingerprints(&[fp3, fp1, fp2])
        );
    }

    #[test]
    fn composite_fingerprint_different_sets_differ() {
        let fp1 = Fingerprint(1);
        let fp2 = Fingerprint(2);
        let fp3 = Fingerprint(3);
        assert_ne!(
            Fingerprint::from_fingerprints(&[fp1, fp2]),
            Fingerprint::from_fingerprints(&[fp2, fp3])
        );
    }

    #[test]
    fn composite_fingerprint_deterministic() {
        let fp1 = Fingerprint(42);
        let fp2 = Fingerprint(99);
        let a = Fingerprint::from_fingerprints(&[fp1, fp2]);
        let b = Fingerprint::from_fingerprints(&[fp1, fp2]);
        assert_eq!(a, b);
    }
}
