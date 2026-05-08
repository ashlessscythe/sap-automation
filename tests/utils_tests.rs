use rand::Rng;
use sap_automation::utils::utils::{decrypt_data, encrypt_data};

#[test]
fn test_encrypt_decrypt() {
    // Generate a random key
    let mut key = vec![0u8; 32]; // 256 bits for AES-256
    rand::thread_rng().fill(&mut key[..]);

    // Test data
    let original_data = "This is a test string for encryption and decryption";

    // Encrypt the data
    let encrypted = encrypt_data(original_data, &key).expect("Failed to encrypt data");

    // Verify that the encrypted data is different from the original
    assert_ne!(encrypted, original_data);

    // Decrypt the data
    let decrypted = decrypt_data(&encrypted, &key).expect("Failed to decrypt data");

    // Verify that the decrypted data matches the original
    assert_eq!(decrypted, original_data);
}

#[test]
fn test_encrypt_decrypt_empty_string() {
    // Generate a random key
    let mut key = vec![0u8; 32]; // 256 bits for AES-256
    rand::thread_rng().fill(&mut key[..]);

    // Test with empty string
    let original_data = "";

    // Encrypt the data
    let encrypted = encrypt_data(original_data, &key).expect("Failed to encrypt empty string");

    // Decrypt the data
    let decrypted = decrypt_data(&encrypted, &key).expect("Failed to decrypt empty string");

    // Verify that the decrypted data matches the original
    assert_eq!(decrypted, original_data);
}

#[test]
fn test_decrypt_with_wrong_key() {
    // Generate two different keys
    let mut key1 = vec![0u8; 32];
    let mut key2 = vec![0u8; 32];
    rand::thread_rng().fill(&mut key1[..]);
    rand::thread_rng().fill(&mut key2[..]);

    // Ensure keys are different
    if key1 == key2 {
        key2[0] = key1[0].wrapping_add(1);
    }

    // Test data
    let original_data = "This is a test string for encryption and decryption";

    // Encrypt with key1
    let encrypted = encrypt_data(original_data, &key1).expect("Failed to encrypt data");

    // Attempt to decrypt with key2 (should fail)
    let result = decrypt_data(&encrypted, &key2);
    assert!(result.is_err(), "Decryption with wrong key should fail");
}

#[test]
fn test_contains_function() {
    use sap_automation::utils::utils::contains;

    // Test case-sensitive contains
    assert!(contains("Hello World", "World", Some(true)));
    assert!(!contains("Hello World", "world", Some(true)));

    // Test case-insensitive contains
    assert!(contains("Hello World", "world", Some(false)));
    assert!(contains("Hello World", "WORLD", Some(false)));

    // Test with default (case-sensitive)
    assert!(contains("Hello World", "World", None));
    assert!(!contains("Hello World", "world", None));
}

#[test]
fn test_mult_contains_function() {
    use sap_automation::utils::utils::mult_contains;

    // Test with multiple needles
    assert!(mult_contains("Hello World", &["Hello", "World"]));
    assert!(mult_contains("Hello World", &["Goodbye", "World"]));
    assert!(!mult_contains("Hello World", &["Goodbye", "Universe"]));

    // Test with empty needles array
    assert!(!mult_contains("Hello World", &[]));

    // Test with empty haystack
    assert!(!mult_contains("", &["Hello", "World"]));
}

#[test]
fn test_generate_timestamp() {
    use sap_automation::utils::utils::generate_timestamp;

    // Get a timestamp
    let timestamp = generate_timestamp();

    // Verify it's a string of the expected length (YYYYMMDDhhmmss = 14 characters)
    assert_eq!(timestamp.len(), 14);

    // Verify it contains only digits
    assert!(timestamp.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn test_version_constant() {
    // Test that the version constant is correctly set from Cargo.toml
    // This test verifies that the env! macro is working correctly
    let version = env!("CARGO_PKG_VERSION");
    
    // Version should not be empty
    assert!(!version.is_empty());
    
    // Version should follow semantic versioning format (e.g., "0.3.2").
    // `str::matches` is literal-substring, not regex, so check the shape
    // explicitly: three dot-separated all-digit segments.
    let parts: Vec<&str> = version.split('.').collect();
    assert!(
        parts.len() >= 3 && parts.iter().take(3).all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "version '{version}' does not look like semver"
    );
    
    // Print the version for verification
    println!("Current version: {}", version);
}
