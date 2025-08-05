#![forbid(unsafe_code)]
#![deny(trivial_casts, trivial_numeric_casts)]

//! Secp256k1 signing scheme for Malachite BFT
//! 
//! This crate provides secp256k1 ECDSA signatures compatible with Ethereum's signing scheme.
//! Addresses are derived using Ethereum's standard: keccak256(pubkey)[12:32]

use std::fmt;

use k256::ecdsa::{SigningKey, VerifyingKey, Signature as EcdsaSignature};
use k256::{PublicKey as K256PublicKey, SecretKey as K256SecretKey};
use signature::{Signer, Verifier};
use thiserror::Error;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Error types for secp256k1 operations
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Secp256k1Error {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Invalid private key")]
    InvalidPrivateKey,
    #[error("Invalid address derivation")]
    InvalidAddress,
}

/// Secp256k1 private key wrapper
#[derive(Clone)]
pub struct PrivateKey {
    inner: SigningKey,
}

impl PrivateKey {
    /// Create a new private key from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Secp256k1Error> {
        let secret_key = K256SecretKey::from_slice(bytes)
            .map_err(|_| Secp256k1Error::InvalidPrivateKey)?;
        Ok(Self {
            inner: SigningKey::from(secret_key),
        })
    }

    /// Create a random private key
    #[cfg(feature = "rand")]
    pub fn generate<R: rand::CryptoRng + rand::RngCore>(rng: &mut R) -> Self {
        Self {
            inner: SigningKey::random(rng),
        }
    }

    /// Get the corresponding public key
    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            inner: *self.inner.verifying_key(),
        }
    }

    /// Sign a message
    pub fn sign(&self, data: &[u8]) -> Signature {
        // Hash the data using Keccak256 (Ethereum standard)
        let hash = keccak256(data);
        let signature: EcdsaSignature = self.inner.sign(&hash);
        Signature { inner: signature }
    }

    /// Get the raw bytes of the private key
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes().into()
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrivateKey").finish_non_exhaustive()
    }
}

/// Secp256k1 public key wrapper
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublicKey {
    inner: VerifyingKey,
}

#[cfg(feature = "serde")]
impl serde::Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.to_uncompressed_bytes();
        serializer.serialize_bytes(&bytes)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        PublicKey::from_uncompressed_bytes(&bytes)
            .map_err(|e| serde::de::Error::custom(format!("Invalid public key: {}", e)))
    }
}

impl PublicKey {
    /// Create a public key from uncompressed bytes (64 bytes: x || y coordinates)
    pub fn from_uncompressed_bytes(bytes: &[u8]) -> Result<Self, Secp256k1Error> {
        if bytes.len() != 64 {
            return Err(Secp256k1Error::InvalidPublicKey);
        }
        
        // Add the 0x04 prefix for uncompressed format
        let mut full_bytes = vec![0x04];
        full_bytes.extend_from_slice(bytes);
        
        let k256_pubkey = K256PublicKey::from_sec1_bytes(&full_bytes)
            .map_err(|_| Secp256k1Error::InvalidPublicKey)?;
        
        Ok(Self {
            inner: VerifyingKey::from(k256_pubkey),
        })
    }

    /// Create a public key from compressed bytes (33 bytes)
    pub fn from_compressed_bytes(bytes: &[u8]) -> Result<Self, Secp256k1Error> {
        let k256_pubkey = K256PublicKey::from_sec1_bytes(bytes)
            .map_err(|_| Secp256k1Error::InvalidPublicKey)?;
        
        Ok(Self {
            inner: VerifyingKey::from(k256_pubkey),
        })
    }

    /// Get the uncompressed bytes (64 bytes: x || y coordinates, no 0x04 prefix)
    pub fn to_uncompressed_bytes(&self) -> [u8; 64] {
        let encoded = self.inner.to_encoded_point(false);
        let bytes = encoded.as_bytes();
        // Skip the 0x04 prefix
        let mut result = [0u8; 64];
        result.copy_from_slice(&bytes[1..]);
        result
    }

    /// Get the compressed bytes (33 bytes)
    pub fn to_compressed_bytes(&self) -> [u8; 33] {
        let encoded = self.inner.to_encoded_point(true);
        let bytes = encoded.as_bytes();
        let mut result = [0u8; 33];
        result.copy_from_slice(bytes);
        result
    }

    /// Derive Ethereum address from public key
    /// Address = keccak256(pubkey)[12:32] (last 20 bytes)
    pub fn to_ethereum_address(&self) -> [u8; 20] {
        let uncompressed = self.to_uncompressed_bytes();
        let hash = keccak256(&uncompressed);
        let mut address = [0u8; 20];
        address.copy_from_slice(&hash[12..32]);
        address
    }

    /// Verify a signature
    pub fn verify(&self, data: &[u8], signature: &Signature) -> Result<(), Secp256k1Error> {
        let hash = keccak256(data);
        self.inner
            .verify(&hash, &signature.inner)
            .map_err(|_| Secp256k1Error::InvalidSignature)
    }

    /// Get raw bytes (uncompressed format for compatibility)
    pub fn as_bytes(&self) -> [u8; 64] {
        self.to_uncompressed_bytes()
    }
}

/// Secp256k1 signature wrapper
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    inner: EcdsaSignature,
}

#[cfg(feature = "serde")]
impl serde::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.to_bytes();
        serializer.serialize_bytes(&bytes)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        Signature::from_bytes(&bytes)
            .map_err(|e| serde::de::Error::custom(format!("Invalid signature: {}", e)))
    }
}

impl Signature {
    /// Create a signature from DER bytes
    pub fn from_der(bytes: &[u8]) -> Result<Self, Secp256k1Error> {
        let signature = EcdsaSignature::from_der(bytes)
            .map_err(|_| Secp256k1Error::InvalidSignature)?;
        Ok(Self { inner: signature })
    }

    /// Get the DER encoding of the signature
    pub fn to_der(&self) -> Vec<u8> {
        self.inner.to_der().as_bytes().to_vec()
    }

    /// Get raw bytes (r || s, 64 bytes)
    pub fn to_bytes(&self) -> [u8; 64] {
        self.inner.to_bytes().into()
    }

    /// Create from raw bytes (r || s, 64 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Secp256k1Error> {
        if bytes.len() != 64 {
            return Err(Secp256k1Error::InvalidSignature);
        }
        
        let signature = EcdsaSignature::from_slice(bytes)
            .map_err(|_| Secp256k1Error::InvalidSignature)?;
        
        Ok(Self { inner: signature })
    }
}

/// Keccak256 hash function (used by Ethereum)
fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let mut rng = rand::thread_rng();
        let private_key = PrivateKey::generate(&mut rng);
        let public_key = private_key.public_key();
        
        // Test round-trip
        let private_bytes = private_key.to_bytes();
        let recovered_private = PrivateKey::from_bytes(&private_bytes).unwrap();
        assert_eq!(private_key.public_key().as_bytes(), recovered_private.public_key().as_bytes());
        
        // Test public key formats
        let uncompressed = public_key.to_uncompressed_bytes();
        let compressed = public_key.to_compressed_bytes();
        
        let recovered_from_uncompressed = PublicKey::from_uncompressed_bytes(&uncompressed).unwrap();
        let recovered_from_compressed = PublicKey::from_compressed_bytes(&compressed).unwrap();
        
        assert_eq!(public_key, recovered_from_uncompressed);
        assert_eq!(public_key, recovered_from_compressed);
    }

    #[test]
    fn test_signing_and_verification() {
        let mut rng = rand::thread_rng();
        let private_key = PrivateKey::generate(&mut rng);
        let public_key = private_key.public_key();
        
        let message = b"Hello, Malachite!";
        let signature = private_key.sign(message);
        
        // Verify with correct key
        assert!(public_key.verify(message, &signature).is_ok());
        
        // Verify with wrong message should fail
        let wrong_message = b"Wrong message";
        assert!(public_key.verify(wrong_message, &signature).is_err());
    }

    #[test]
    fn test_ethereum_address_derivation() {
        let mut rng = rand::thread_rng();
        let private_key = PrivateKey::generate(&mut rng);
        let public_key = private_key.public_key();
        
        let address = public_key.to_ethereum_address();
        assert_eq!(address.len(), 20);
        
        // Address should be deterministic
        let address2 = public_key.to_ethereum_address();
        assert_eq!(address, address2);
    }

    #[test]
    fn test_signature_serialization() {
        let mut rng = rand::thread_rng();
        let private_key = PrivateKey::generate(&mut rng);
        let message = b"test message";
        let signature = private_key.sign(message);
        
        // Test DER round-trip
        let der_bytes = signature.to_der();
        let recovered_sig = Signature::from_der(&der_bytes).unwrap();
        assert_eq!(signature, recovered_sig);
        
        // Test raw bytes round-trip
        let raw_bytes = signature.to_bytes();
        let recovered_sig2 = Signature::from_bytes(&raw_bytes).unwrap();
        assert_eq!(signature, recovered_sig2);
    }
}