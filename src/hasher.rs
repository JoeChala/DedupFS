use sha2::{Digest, Sha256};

pub trait Hasher {
    fn hash(&self, data: &[u8]) -> String;
}
#[derive(Clone, Copy)]
pub struct Sha256Hasher;

impl Hasher for Sha256Hasher {
    fn hash(&self, data: &[u8]) -> String {
        let digest = Sha256::digest(data);

        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_expected_hash() {
        let hasher = Sha256Hasher;

        let hash = hasher.hash(b"hello");

        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn produces_same_hash_for_same_input() {
        let hasher = Sha256Hasher;

        let first = hasher.hash(b"dedupfs");
        let second = hasher.hash(b"dedupfs");

        assert_eq!(first, second);
    }

    #[test]
    fn produces_different_hashes_for_different_input() {
        let hasher = Sha256Hasher;

        let first = hasher.hash(b"hello");
        let second = hasher.hash(b"world");

        assert_ne!(first, second);
    }

    #[test]
    fn hashes_empty_input() {
        let hasher = Sha256Hasher;

        let hash = hasher.hash(b"");

        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn produces_64_character_hex_string() {
        let hasher = Sha256Hasher;

        let hash = hasher.hash(b"dedupfs");

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|character| character.is_ascii_hexdigit()));
    }
}
