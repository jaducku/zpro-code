//! API key encryption for config file storage.
//! Uses XOR cipher with a machine-specific key derived from hostname + username.
//! This is obfuscation (not cryptographic security) — prevents casual plaintext exposure.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

/// Encrypt an API key for storage in config.json
pub fn encrypt_api_key(key: &str) -> String {
    let machine_key = derive_machine_key();
    let encrypted = xor_bytes(key.as_bytes(), &machine_key);
    format!("enc:{}", BASE64.encode(&encrypted))
}

/// Decrypt an API key from config.json
pub fn decrypt_api_key(stored: &str) -> Option<String> {
    if let Some(encoded) = stored.strip_prefix("enc:") {
        let encrypted = BASE64.decode(encoded).ok()?;
        let machine_key = derive_machine_key();
        let decrypted = xor_bytes(&encrypted, &machine_key);
        String::from_utf8(decrypted).ok()
    } else {
        // Not encrypted (legacy plaintext) — return as-is
        Some(stored.to_string())
    }
}

/// Derive a machine-specific key from hostname + username.
/// Not cryptographically secure, but ties the encrypted value to this machine.
fn derive_machine_key() -> Vec<u8> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "atom".to_string());
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string());
    let seed = format!("zpro-code:{}:{}", hostname, username);

    // Simple hash to get key bytes
    let mut key = Vec::new();
    let mut hash: u64 = 5381;
    for byte in seed.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        key.push((hash & 0xFF) as u8);
    }
    // Extend to at least 128 bytes
    while key.len() < 128 {
        hash = hash.wrapping_mul(33).wrapping_add(key.len() as u64);
        key.push((hash & 0xFF) as u8);
    }
    key
}

fn xor_bytes(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let original = "sk-or-v1-abc123def456";
        let encrypted = encrypt_api_key(original);
        assert!(encrypted.starts_with("enc:"));
        assert_ne!(encrypted, original);

        let decrypted = decrypt_api_key(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_plaintext_passthrough() {
        let plain = "sk-plain-key";
        let result = decrypt_api_key(plain).unwrap();
        assert_eq!(result, plain);
    }

    #[test]
    fn test_encrypted_not_readable() {
        let encrypted = encrypt_api_key("my-secret-key");
        assert!(!encrypted.contains("my-secret-key"));
    }
}
