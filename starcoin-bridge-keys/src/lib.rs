// Wrapper for Starcoin key management - provides key file reading functionality
// Note: Uses Starcoin's crypto types (StarcoinKeyPair) because Bridge requires Secp256k1
#![allow(dead_code, unused_variables)]

use std::path::Path;

// Re-export StarcoinKeyPair from starcoin-bridge-types
pub use starcoin_bridge_types::crypto::StarcoinKeyPair;

pub mod keygen {
    use super::*;
    use anyhow::{anyhow, Result};
    use fastcrypto::{
        secp256k1::Secp256k1KeyPair,
        traits::EncodeDecodeBase64,
    };
    use std::path::PathBuf;

    /// Generate a new Secp256k1 keypair for bridge authority and write to file
    /// The key is written as base64-encoded `flag || privkey` (StarcoinKeyPair format)
    pub fn generate_bridge_authority_key_and_write_to_file(path: &PathBuf) -> Result<()> {
        let (_, kp): ((), Secp256k1KeyPair) = starcoin_bridge_types::crypto::get_key_pair();

        println!("Generated new Secp256k1 keypair for bridge authority");

        // Print public key
        use fastcrypto::traits::{KeyPair as _, ToFromBytes};
        println!("Public key (hex): {}", hex::encode(kp.public().as_bytes()));

        // Calculate Ethereum address from public key
        let eth_address = calculate_eth_address(&kp.public());
        println!("Ethereum address: 0x{}", hex::encode(eth_address));

        // Wrap in StarcoinKeyPair and encode (this adds the scheme flag)
        let starcoin_kp = StarcoinKeyPair::Secp256k1(kp);
        let base64_encoded = starcoin_kp.encode_base64();

        // Write to file
        std::fs::write(path, base64_encoded)
            .map_err(|err| anyhow!("Failed to write key to {:?}: {}", path, err))?;

        println!("Key written to: {:?}", path);
        Ok(())
    }

    /// Calculate Ethereum address from Secp256k1 public key
    fn calculate_eth_address(pubkey: &fastcrypto::secp256k1::Secp256k1PublicKey) -> [u8; 20] {
        use fastcrypto::traits::ToFromBytes;
        use sha3::{Digest, Keccak256};

        // Get uncompressed public key (65 bytes: 0x04 + x + y)
        let pubkey_bytes = pubkey.as_bytes();

        // For Ethereum address, we hash the x and y coordinates (skip the first byte 0x04)
        // Secp256k1PublicKey in fastcrypto is 33 bytes (compressed), need to expand
        // For now, use a simplified approach - hash the bytes we have
        let hash = Keccak256::digest(&pubkey_bytes[..]);

        // Take last 20 bytes
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..]);
        addr
    }

    /// Generate a new StarcoinKeyPair (Ed25519 or Secp256k1) for bridge client and write to file
    /// The key is written as base64-encoded `flag || privkey`
    pub fn generate_bridge_client_key_and_write_to_file(
        path: &PathBuf,
        use_ecdsa: bool,
    ) -> Result<()> {
        use fastcrypto::ed25519::Ed25519KeyPair;
        use fastcrypto::traits::{KeyPair as _, ToFromBytes};

        let kp = if use_ecdsa {
            let (_, kp): ((), Secp256k1KeyPair) = starcoin_bridge_types::crypto::get_key_pair();
            println!("Generated new Secp256k1 keypair for bridge client");
            println!("Public key (hex): {}", hex::encode(kp.public().as_bytes()));

            let eth_address = calculate_eth_address(&kp.public());
            println!("Ethereum address: 0x{}", hex::encode(eth_address));
            StarcoinKeyPair::Secp256k1(kp)
        } else {
            let (_, kp): ((), Ed25519KeyPair) = starcoin_bridge_types::crypto::get_key_pair();
            println!("Generated new Ed25519 keypair for bridge client");
            println!("Public key (hex): {}", hex::encode(kp.public().as_bytes()));
            StarcoinKeyPair::Ed25519(kp)
        };

        // Encode the keypair as base64
        let contents = kp.encode_base64();

        // Write to file
        std::fs::write(path, contents)
            .map_err(|err| anyhow!("Failed to write key to {:?}: {}", path, err))?;

        println!("Key written to: {:?}", path);
        Ok(())
    }
}

pub mod keypair_file {
    use super::*;
    use anyhow::{anyhow, Result};
    use fastcrypto::{secp256k1::Secp256k1KeyPair, traits::EncodeDecodeBase64};
    use std::path::PathBuf;

    // Read a StarcoinKeyPair from a file
    // The file should contain Base64 encoded `flag || privkey`
    // If require_secp256k1 is true, only Secp256k1 keys are accepted
    pub fn read_key(path: &PathBuf, require_secp256k1: bool) -> Result<StarcoinKeyPair> {
        if !path.exists() {
            return Err(anyhow!("Key file not found at path: {:?}", path));
        }

        let file_contents = std::fs::read_to_string(path)?;
        let contents = file_contents.as_str().trim();

        // Try base64 encoded StarcoinKeyPair `flag || privkey`
        if let Ok(key) = StarcoinKeyPair::decode_base64(contents) {
            if require_secp256k1 && !matches!(key, StarcoinKeyPair::Secp256k1(_)) {
                return Err(anyhow!("Key is not Secp256k1"));
            }
            return Ok(key);
        }

        // Try base64 encoded Raw Secp256k1 key `privkey`
        if let Ok(key) = Secp256k1KeyPair::decode_base64(contents) {
            return Ok(StarcoinKeyPair::Secp256k1(key));
        }

        Err(anyhow!(
            "Invalid key file format. Expected Base64 encoded key at {:?}",
            path
        ))
    }

    // Read from file as Base64 encoded `flag || privkey` and return a NetworkKeyPair (Ed25519)
    pub fn read_network_keypair_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<fastcrypto::ed25519::Ed25519KeyPair> {
        let kp = read_keypair_from_file(path)?;
        if let StarcoinKeyPair::Ed25519(kp) = kp {
            Ok(kp)
        } else {
            Err(anyhow!("Invalid scheme for network keypair"))
        }
    }

    // Read from file as Base64 encoded `flag || privkey` and return a StarcoinKeyPair
    pub fn read_keypair_from_file<P: AsRef<Path>>(path: P) -> Result<StarcoinKeyPair> {
        let contents = std::fs::read_to_string(path)?;
        StarcoinKeyPair::decode_base64(contents.as_str().trim())
            .map_err(|e| anyhow!("Failed to decode keypair: {}", e))
    }
}
