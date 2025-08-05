use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::ethereum_rpc::EthereumRPC;

/// Simplified representation of a validator from the staking contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingValidator {
    pub pubkey: Vec<u8>,    // 64 bytes uncompressed secp256k1 pubkey (x||y)
    pub power: u64,         // Voting power (stake / 1e18)
    pub address: String,    // Ethereum address (0x...)
}

/// Staking contract interface for reading validator sets
pub struct StakingContract {
    rpc: EthereumRPC,
    contract_address: String,
}

impl StakingContract {
    /// Create a new staking contract interface
    pub fn new(rpc: EthereumRPC, contract_address: String) -> Self {
        Self {
            rpc,
            contract_address,
        }
    }

    /// Get the next validator set (for H+1 semantics) at a specific block
    pub async fn get_next_validator_set(&self, block_tag: &str) -> Result<Vec<StakingValidator>> {
        // Call getNextValidatorSet() method
        let call_data = "0x38c50ab8"; // getNextValidatorSet() method signature
        
        let params = json!([
            {
                "to": self.contract_address,
                "data": call_data
            },
            block_tag
        ]);

        let result: serde_json::Value = self.rpc.rpc_request(
            "eth_call", 
            params, 
            std::time::Duration::from_secs(30)
        ).await?;
        
        // Parse the result - expecting (bytes[] pubkeys, uint64[] powers, address[] addresses)
        let hex_result = result.as_str()
            .ok_or_else(|| eyre!("Invalid response from eth_call"))?;
            
        if hex_result == "0x" || hex_result.len() < 3 {
            // No validators
            return Ok(vec![]);
        }
        
        // Parse the ABI-encoded response
        self.parse_validator_set_response(hex_result)
    }

    /// Get the current validator set at a specific block
    pub async fn get_current_validator_set(&self, block_tag: &str) -> Result<Vec<StakingValidator>> {
        // Call getCurrentValidatorSet() method
        let call_data = "0x0940572e"; // getCurrentValidatorSet() method signature
        
        let params = json!([
            {
                "to": self.contract_address,
                "data": call_data
            },
            block_tag
        ]);

        let result: serde_json::Value = self.rpc.rpc_request(
            "eth_call", 
            params, 
            std::time::Duration::from_secs(30)
        ).await?;
        
        // Parse the result
        let hex_result = result.as_str()
            .ok_or_else(|| eyre!("Invalid response from eth_call"))?;
            
        if hex_result == "0x" || hex_result.len() < 3 {
            // No validators
            return Ok(vec![]);
        }
        
        // Parse the ABI-encoded response
        self.parse_validator_set_response(hex_result)
    }

    /// Parse the ABI-encoded response from getNextValidatorSet() or getCurrentValidatorSet()
    /// Format: (bytes[] pubkeys, uint64[] powers, address[] addresses)
    fn parse_validator_set_response(&self, hex_data: &str) -> Result<Vec<StakingValidator>> {
        // Remove 0x prefix
        let data = hex_data.strip_prefix("0x").unwrap_or(hex_data);
        
        if data.len() < 64 {
            return Ok(vec![]);
        }
        
        // For simplicity in the PoC, we'll implement a basic ABI decoder
        // In production, you'd use a proper ABI library like alloy-sol-types
        
        // Skip method signature (first 4 bytes = 8 hex chars)
        let response_data = &data[8..];
        
        // Parse the tuple (bytes[], uint64[], address[])
        // This is a simplified parser - in practice you'd use alloy-sol-types
        let validators = self.simple_abi_decode(response_data)?;
        
        Ok(validators)
    }
    
    /// Simplified ABI decoder for the validator set response
    /// This implements basic ABI decoding for (bytes[], uint64[], address[])
    fn simple_abi_decode(&self, data: &str) -> Result<Vec<StakingValidator>> {
        if data.len() < 128 {
            // Not enough data for the tuple structure
            return Ok(vec![]);
        }
        
        // Parse hex data
        let bytes = hex::decode(data).map_err(|e| eyre!("Failed to decode hex: {}", e))?;
        if bytes.len() < 96 {
            return Ok(vec![]);
        }
        
        // ABI structure: (bytes[], uint64[], address[])
        // Skip to the data section (first 32 bytes are offsets to arrays)
        let mut offset = 0;
        
        // Read offset to pubkeys array (first 32 bytes)
        let pubkeys_offset = self.read_u256(&bytes, offset)? as usize;
        offset += 32;
        
        // Read offset to powers array (next 32 bytes)  
        let powers_offset = self.read_u256(&bytes, offset)? as usize;
        offset += 32;
        
        // Read offset to addresses array (next 32 bytes)
        let addresses_offset = self.read_u256(&bytes, offset)? as usize;
        
        // Parse arrays at their respective offsets
        let pubkeys = self.parse_bytes_array(&bytes, pubkeys_offset)?;
        let powers = self.parse_uint64_array(&bytes, powers_offset)?;
        let addresses = self.parse_address_array(&bytes, addresses_offset)?;
        
        // Combine into validator structs
        let mut validators = Vec::new();
        let min_len = pubkeys.len().min(powers.len()).min(addresses.len());
        
        for i in 0..min_len {
            validators.push(StakingValidator {
                pubkey: pubkeys[i].clone(),
                power: powers[i],
                address: addresses[i].clone(),
            });
        }
        
        Ok(validators)
    }
    
    /// Helper to read a U256 value from bytes at offset
    fn read_u256(&self, bytes: &[u8], offset: usize) -> Result<u64> {
        if offset + 32 > bytes.len() {
            return Err(eyre!("Not enough bytes to read U256"));
        }
        
        // Convert last 8 bytes to u64 (big endian)
        let mut value_bytes = [0u8; 8];
        value_bytes.copy_from_slice(&bytes[offset + 24..offset + 32]);
        Ok(u64::from_be_bytes(value_bytes))
    }
    
    /// Parse bytes[] array from ABI data
    fn parse_bytes_array(&self, bytes: &[u8], offset: usize) -> Result<Vec<Vec<u8>>> {
        if offset + 32 > bytes.len() {
            return Ok(vec![]);
        }
        
        let length = self.read_u256(bytes, offset)? as usize;
        let mut result = Vec::new();
        let mut current_offset = offset + 32;
        
        for _ in 0..length {
            if current_offset + 32 > bytes.len() {
                break;
            }
            
            // Read offset to this bytes element
            let element_offset = offset + self.read_u256(bytes, current_offset)? as usize;
            current_offset += 32;
            
            if element_offset + 32 <= bytes.len() {
                // Read length of this bytes element
                let element_length = self.read_u256(bytes, element_offset)? as usize;
                let data_start = element_offset + 32;
                
                if data_start + element_length <= bytes.len() {
                    result.push(bytes[data_start..data_start + element_length].to_vec());
                }
            }
        }
        
        Ok(result)
    }
    
    /// Parse uint64[] array from ABI data
    fn parse_uint64_array(&self, bytes: &[u8], offset: usize) -> Result<Vec<u64>> {
        if offset + 32 > bytes.len() {
            return Ok(vec![]);
        }
        
        let length = self.read_u256(bytes, offset)? as usize;
        let mut result = Vec::new();
        let mut current_offset = offset + 32;
        
        for _ in 0..length {
            if current_offset + 32 > bytes.len() {
                break;
            }
            
            let value = self.read_u256(bytes, current_offset)?;
            result.push(value);
            current_offset += 32;
        }
        
        Ok(result)
    }
    
    /// Parse address[] array from ABI data  
    fn parse_address_array(&self, bytes: &[u8], offset: usize) -> Result<Vec<String>> {
        if offset + 32 > bytes.len() {
            return Ok(vec![]);
        }
        
        let length = self.read_u256(bytes, offset)? as usize;
        let mut result = Vec::new();
        let mut current_offset = offset + 32;
        
        for _ in 0..length {
            if current_offset + 32 > bytes.len() {
                break;
            }
            
            // Address is the last 20 bytes of the 32-byte slot
            let address_bytes = &bytes[current_offset + 12..current_offset + 32];
            let address = format!("0x{}", hex::encode(address_bytes));
            result.push(address);
            current_offset += 32;
        }
        
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staking_validator_serialization() {
        let validator = StakingValidator {
            pubkey: vec![1u8; 64],
            power: 1000,
            address: "0x742d35Cc6532C02532985A5C7E27A73e9e3d8f59".to_string(),
        };
        
        let json = serde_json::to_string(&validator).unwrap();
        let deserialized: StakingValidator = serde_json::from_str(&json).unwrap();
        
        assert_eq!(validator.power, deserialized.power);
        assert_eq!(validator.address, deserialized.address);
        assert_eq!(validator.pubkey, deserialized.pubkey);
    }
}