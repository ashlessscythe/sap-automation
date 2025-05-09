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

use crate::utils::config_types::SapConfig;
use chrono::{DateTime, Utc, TimeZone};
use chrono_tz::Tz;
use std::str::FromStr;

// Generate timestamp
pub fn generate_timestamp() -> String {
    // Get the timezone from config or use default ("UTC")
    let timezone_str = match SapConfig::load() {
        Ok(config) => {
            if let Some(global) = &config.global {
                global.timezone.clone()
            } else {
                "UTC".to_string()
            }
        },
        Err(_) => "UTC".to_string(),
    };
    
    // Get current UTC time
    let now = Utc::now();
    println!("Now is: {} UTC", now);
    println!("Using timezone: {}", timezone_str);
    
    // Format the timestamp with the appropriate timezone offset
    let timestamp = apply_timezone(now, &timezone_str);
    println!("Generated timestamp: {}", timestamp);
    
    timestamp
}

/// Apply timezone to a UTC datetime and return a formatted timestamp string
/// 
/// Supports:
/// - Standard timezone names like "UTC", "MST", "MDT", "EST", etc.
/// - IANA timezone database names like "America/Denver", "Europe/London", etc.
/// - Legacy numeric offsets like "-7" (for backward compatibility)
pub fn apply_timezone(utc_time: DateTime<Utc>, timezone_str: &str) -> String {
    // First try to parse as a numeric offset (for backward compatibility)
    if let Ok(hours_offset) = timezone_str.parse::<i32>() {
        println!("Using numeric offset: {} hours", hours_offset);
        let adjusted_time = utc_time + chrono::Duration::hours(hours_offset as i64);
        println!("Adjusted time: {}", adjusted_time);
        return adjusted_time.format("%Y%m%d%H%M%S").to_string();
    }
    
    // Try to parse as a chrono-tz timezone
    if let Ok(tz) = Tz::from_str(timezone_str) {
        println!("Using timezone database entry: {}", tz);
        // Convert UTC time to the target timezone and format it
        let local_time = utc_time.with_timezone(&tz);
        println!("Local time in {}: {}", tz, local_time);
        return local_time.format("%Y%m%d%H%M%S").to_string();
    }
    
    // Handle common abbreviations not directly supported by chrono-tz
    let tz_string = match timezone_str.to_uppercase().as_str() {
        "UTC" | "GMT" => {
            println!("Using UTC/GMT timezone");
            "UTC"
        },
        "EST" | "EDT" => {
            println!("Converting {} to America/New_York", timezone_str);
            "America/New_York"
        },
        "CST" | "CDT" => {
            println!("Converting {} to America/Chicago", timezone_str);
            "America/Chicago"
        },
        "MST" | "MDT" => {
            println!("Converting {} to America/Denver", timezone_str);
            "America/Denver"
        },
        "PST" | "PDT" => {
            println!("Converting {} to America/Los_Angeles", timezone_str);
            "America/Los_Angeles"
        },
        _ => {
            // If we can't parse the timezone, fall back to UTC
            println!("Warning: Unknown timezone '{}', falling back to UTC", timezone_str);
            return utc_time.format("%Y%m%d%H%M%S").to_string();
        }
    };
    
    // Parse the timezone and apply it
    if let Ok(tz) = Tz::from_str(tz_string) {
        println!("Using mapped timezone: {}", tz);
        let local_time = utc_time.with_timezone(&tz);
        println!("Local time in {}: {}", tz, local_time);
        return local_time.format("%Y%m%d%H%M%S").to_string();
    }
    
    // Final fallback to UTC
    println!("Final fallback to UTC");
    utc_time.format("%Y%m%d%H%M%S").to_string()
}
