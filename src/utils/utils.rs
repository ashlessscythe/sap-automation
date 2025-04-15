use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::Rng;
use windows::core::Result;

// keyfile
pub const KEY_FILE_SUFFIX: &str = "_key.bin";

// Encryption/Decryption functions
pub fn encrypt_data(data: &str, key: &[u8]) -> Result<String> {
    // Create a new AES-GCM cipher with the provided key
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };

    // Generate a random nonce (12 bytes for AES-GCM)
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt the data
    let plaintext = data.as_bytes();
    let ciphertext = match cipher.encrypt(nonce, plaintext.as_ref()) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };

    // Combine nonce and ciphertext and encode as base64
    let mut combined = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(general_purpose::STANDARD.encode(combined))
}

pub fn decrypt_data(encrypted_data: &str, key: &[u8]) -> Result<String> {
    // Decode the base64 data
    let combined = match general_purpose::STANDARD.decode(encrypted_data) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };

    // Split into nonce and ciphertext
    if combined.len() < 12 {
        return Err(windows::core::Error::from_win32());
    }

    let nonce_bytes = &combined[..12];
    let ciphertext = &combined[12..];

    // Create cipher with the key
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };

    // Decrypt the data
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = match cipher.decrypt(nonce, ciphertext.as_ref()) {
        Ok(p) => p,
        Err(_) => return Err(windows::core::Error::from_win32()),
    };

    // Convert back to string
    match String::from_utf8(plaintext) {
        Ok(s) => Ok(s),
        Err(_) => Err(windows::core::Error::from_win32()),
    }
}

// Helper function to check if a string contains a substring
pub fn contains(haystack: &str, needle: &str, case_sensitive: Option<bool>) -> bool {
    let case_sensitive_val = case_sensitive.unwrap_or(true);

    if case_sensitive_val {
        haystack.contains(needle)
    } else {
        haystack.to_lowercase().contains(&needle.to_lowercase())
    }
}

// Helper function to check if multiple strings contain a target
pub fn mult_contains(haystack: &str, needles: &[&str]) -> bool {
    for needle in needles {
        if haystack.contains(needle) {
            return true;
        }
    }
    false
}

// Generate timestamp
pub fn generate_timestamp() -> String {
    let now = chrono::Utc::now();
    now.format("%Y%m%d%H%M%S").to_string()
}
