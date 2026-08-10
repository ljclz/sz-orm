use std::collections::HashMap;
use sz_orm_crypto::*;

#[test]
fn test_sha256_known_vector() {
    let result = sha256(b"hello world");
    let hex = sha256_hex(b"hello world");
    assert_eq!(result.len(), 32);
    assert_eq!(
        hex,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn test_sha256_empty() {
    let result = sha256(b"");
    assert_eq!(result.len(), 32);
    let hex = sha256_hex(b"");
    assert_eq!(
        hex,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_sha256_consistency() {
    let data = b"test data for consistency";
    assert_eq!(sha256(data), sha256(data));
}

#[test]
fn test_hmac_sha256_known_vector() {
    let key = b"key";
    let message = b"The quick brown fox jumps over the lazy dog";
    let result = hmac_sha256_hex(key, message);
    assert_eq!(
        result,
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
}

#[test]
fn test_hmac_sha256_empty_message() {
    let result = hmac_sha256(b"key", b"");
    assert_eq!(result.len(), 32);
}

#[test]
fn test_hmac_sha256_empty_key() {
    let result = hmac_sha256(b"", b"message");
    assert_eq!(result.len(), 32);
}

#[test]
fn test_hmac_sha256_consistency() {
    let key = b"secret";
    let msg = b"message";
    assert_eq!(hmac_sha256(key, msg), hmac_sha256(key, msg));
}

#[test]
fn test_aes_gcm_encrypt_decrypt() {
    let crypter = AesGcmCrypter::from_key_str("test-secret-key");
    let plaintext = b"hello, world!";
    let ciphertext = crypter.encrypt(plaintext).unwrap();
    assert_ne!(&ciphertext[..], &plaintext[..]);
    let decrypted = crypter.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_aes_gcm_empty_plaintext() {
    let crypter = AesGcmCrypter::from_key_str("key");
    let ciphertext = crypter.encrypt(b"").unwrap();
    let decrypted = crypter.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, b"");
}

#[test]
fn test_aes_gcm_decrypt_too_short() {
    let crypter = AesGcmCrypter::from_key_str("key");
    let result = crypter.decrypt(b"short");
    assert!(result.is_err());
}

#[test]
fn test_aes_gcm_with_aad() {
    let crypter = AesGcmCrypter::from_key_str("key");
    let plaintext = b"secret data";
    let aad = b"associated data";
    let ciphertext = crypter.encrypt_with_aad(plaintext, aad).unwrap();
    let decrypted = crypter.decrypt_with_aad(&ciphertext, aad).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_aes_gcm_with_aad_wrong_aad() {
    let crypter = AesGcmCrypter::from_key_str("key");
    let plaintext = b"secret data";
    let ciphertext = crypter.encrypt_with_aad(plaintext, b"correct_aad").unwrap();
    let result = crypter.decrypt_with_aad(&ciphertext, b"wrong_aad");
    assert!(result.is_err());
}

#[test]
fn test_aes_gcm_different_nonce_each_encrypt() {
    let crypter = AesGcmCrypter::from_key_str("key");
    let ct1 = crypter.encrypt(b"data").unwrap();
    let ct2 = crypter.encrypt(b"data").unwrap();
    assert_ne!(ct1, ct2);
}

#[test]
fn test_pbkdf2_hash_and_verify() {
    let hasher = Pbkdf2Hasher::new();
    let hash = hasher.hash("mypassword").unwrap();
    assert!(hash.starts_with('$'));
    assert!(hasher.verify("mypassword", &hash).unwrap());
}

#[test]
fn test_pbkdf2_verify_wrong_password() {
    let hasher = Pbkdf2Hasher::new();
    let hash = hasher.hash("correct").unwrap();
    assert!(!hasher.verify("wrong", &hash).unwrap());
}

#[test]
fn test_pbkdf2_empty_password_rejected() {
    let hasher = Pbkdf2Hasher::new();
    assert!(hasher.hash("").is_err());
}

#[test]
fn test_pbkdf2_invalid_hash_format() {
    let hasher = Pbkdf2Hasher::new();
    assert!(hasher.verify("pass", "invalid").is_err());
    assert!(hasher.verify("pass", "$$$").is_err());
}

#[test]
fn test_pbkdf2_with_iterations() {
    let hasher = Pbkdf2Hasher::with_iterations(1000);
    let hash = hasher.hash("test").unwrap();
    assert!(hash.starts_with("$1000$"));
    assert!(hasher.verify("test", &hash).unwrap());
}

#[test]
fn test_hmac_signer_sign_and_verify() {
    let signer = HmacSigner::new();
    let mut params = HashMap::new();
    params.insert("b".to_string(), "2".to_string());
    params.insert("a".to_string(), "1".to_string());
    let sig = signer.sign(&params, "secret");
    assert!(signer.verify(&params, "secret", &sig));
}

#[test]
fn test_hmac_signer_verify_wrong_signature() {
    let signer = HmacSigner::new();
    let params = HashMap::new();
    let _sig = signer.sign(&params, "secret");
    assert!(!signer.verify(&params, "secret", "wrong_signature"));
}

#[test]
fn test_hmac_signer_empty_params() {
    let signer = HmacSigner::new();
    let params = HashMap::new();
    let sig = signer.sign(&params, "secret");
    assert!(!sig.is_empty());
    assert!(signer.verify(&params, "secret", &sig));
}

#[test]
fn test_hmac_signature_verifier() {
    let verifier = HmacSignatureVerifier::from_key_str("my-secret");
    let message = b"test message";
    let signature = verifier.sign(message);
    assert!(verifier.verify(message, &signature));
    assert!(!verifier.verify(message, b"wrong"));
}

#[test]
fn test_hmac_signature_verifier_empty_message() {
    let verifier = HmacSignatureVerifier::new(b"key");
    let sig = verifier.sign(b"");
    assert!(verifier.verify(b"", &sig));
}

#[test]
fn test_key_rotation_basic() {
    let mut mgr = KeyRotationManager::new(3);
    mgr.rotate_key(b"key1".to_vec());
    assert_eq!(mgr.current_version(), 1);
    let (version, sig) = mgr.sign(b"message");
    assert_eq!(version, 1);
    assert!(!sig.is_empty());
    assert!(mgr.verify(b"message", version, &sig));
}

#[test]
fn test_key_rotation_multiple_versions() {
    let mut mgr = KeyRotationManager::new(3);
    mgr.rotate_key(b"key1".to_vec());
    let (v1, sig1) = mgr.sign(b"msg");
    mgr.rotate_key(b"key2".to_vec());
    let (v2, sig2) = mgr.sign(b"msg");
    assert_eq!(v1, 1);
    assert_eq!(v2, 2);
    assert!(mgr.verify(b"msg", v1, &sig1));
    assert!(mgr.verify(b"msg", v2, &sig2));
}

#[test]
fn test_key_rotation_max_versions_eviction() {
    let mut mgr = KeyRotationManager::new(2);
    mgr.rotate_key(b"key1".to_vec());
    let (v1, sig1) = mgr.sign(b"msg");
    mgr.rotate_key(b"key2".to_vec());
    mgr.rotate_key(b"key3".to_vec());
    assert_eq!(mgr.version_count(), 2);
    assert!(!mgr.verify(b"msg", v1, &sig1));
}

#[test]
fn test_key_rotation_with_initial_key() {
    let mgr = KeyRotationManager::with_initial_key(b"initial".to_vec());
    assert_eq!(mgr.current_version(), 1);
    assert_eq!(mgr.version_count(), 1);
}

#[test]
fn test_key_rotation_empty_sign() {
    let mgr = KeyRotationManager::new(3);
    let (version, sig) = mgr.sign(b"msg");
    assert_eq!(version, 0);
    assert!(sig.is_empty());
}

#[test]
fn test_key_rotation_verify_nonexistent_version() {
    let mut mgr = KeyRotationManager::new(3);
    mgr.rotate_key(b"key1".to_vec());
    assert!(!mgr.verify(b"msg", 99, b"sig"));
}

#[test]
fn test_rsa_oaep_encrypt_decrypt() {
    let crypter = RsaOaepCrypter::generate(2048).unwrap();
    let plaintext = b"RSA encrypted message";
    let ciphertext = crypter.encrypt(plaintext).unwrap();
    let decrypted = crypter.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rsa_oaep_empty_plaintext() {
    let crypter = RsaOaepCrypter::generate(2048).unwrap();
    let ciphertext = crypter.encrypt(b"").unwrap();
    let decrypted = crypter.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, b"");
}
