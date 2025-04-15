use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::Rng;
use std::io::{Error, ErrorKind};
use std::result::Result as StdResult;

// Encryption and decryption utilities
// These were in the original sap_utils.rs but weren't being used in the functions
// Adding them here for completeness and future use

pub fn encrypt_data(data: &str, key: &[u8]) -> StdResult<String, Error> {
    let mut rng = rand::thread_rng();
    let nonce_bytes: [u8; 12] = rng.gen();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| Error::new(ErrorKind::Other, "Failed to create cipher"))?;

    let ciphertext = cipher
        .encrypt(nonce, data.as_bytes())
        .map_err(|_| Error::new(ErrorKind::Other, "Encryption failed"))?;

    // Combine nonce and ciphertext and encode with base64
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(general_purpose::STANDARD.encode(combined))
}

pub fn decrypt_data(encrypted_data: &str, key: &[u8]) -> StdResult<String, Error> {
    // Decode base64
    let combined = general_purpose::STANDARD
        .decode(encrypted_data)
        .map_err(|_| Error::new(ErrorKind::Other, "Base64 decoding failed"))?;

    if combined.len() < 12 {
        return Err(Error::new(ErrorKind::Other, "Invalid encrypted data"));
    }

    // Split into nonce and ciphertext
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| Error::new(ErrorKind::Other, "Failed to create cipher"))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| Error::new(ErrorKind::Other, "Decryption failed"))?;

    String::from_utf8(plaintext).map_err(|_| Error::new(ErrorKind::Other, "UTF-8 decoding failed"))
}

pub fn generate_key() -> [u8; 32] {
    let mut rng = rand::thread_rng();
    let mut key = [0u8; 32];
    rng.fill(&mut key);
    key
}
