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
    /// This is a basic implementation for the PoC
    fn simple_abi_decode(&self, _data: &str) -> Result<Vec<StakingValidator>> {
        // For the PoC, return a placeholder validator set
        // In practice, you would implement proper ABI decoding here
        // or use alloy-sol-types for automatic ABI encoding/decoding
        
        // TODO: Implement proper ABI decoding
        // For now, return an empty set to avoid compilation errors
        Ok(vec![])
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