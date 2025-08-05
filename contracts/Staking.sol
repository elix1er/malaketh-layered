// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title Staking Contract
/// @notice Cosmos-style validator registry with H+1 activation and 3w unbonding
/// @dev Cheap & simple for a PoC: linear scans in view functions are acceptable
contract Staking {
    /// @notice Maximum number of active validators (Cosmos default)
    uint256 public constant MAX_VALIDATORS = 100;
    
    /// @notice Minimum self-delegation requirement (1 full token)
    uint256 public constant MIN_SELF_DELEGATION = 1e18;
    
    /// @notice Unbonding period in seconds (3 weeks, Cosmos default)
    uint256 public constant UNBONDING_PERIOD = 21 days;
    
    /// @notice Power reduction factor (wei to voting power)
    uint256 public constant POWER_REDUCTION = 1e18;

    /// @dev Validator information
    struct Validator {
        bytes pubkeyXY;        // 64 bytes: x||y coordinates (no 0x04 prefix)
        uint256 stake;         // total staked amount in wei
        bool exists;           // whether validator is registered
    }
    
    /// @dev Unbonding entry
    struct UnbondingEntry {
        uint256 amount;        // amount being unbonded
        uint256 releaseTime;   // when funds can be withdrawn
    }

    /// @notice Mapping from validator address to validator info
    mapping(address => Validator) public validators;
    
    /// @notice Array of all validator addresses for iteration
    address[] public validatorAddresses;
    
    /// @notice Mapping from validator to unbonding queue
    mapping(address => UnbondingEntry[]) public unbondingQueue;

    /// @notice Emitted when a validator registers
    event ValidatorRegistered(address indexed validator, bytes pubkeyXY);
    
    /// @notice Emitted when a validator bonds tokens
    event ValidatorBonded(address indexed validator, uint256 amount, uint256 newTotal);
    
    /// @notice Emitted when a validator starts unbonding
    event ValidatorUnbonded(address indexed validator, uint256 amount, uint256 releaseTime);
    
    /// @notice Emitted when a validator withdraws unbonded tokens
    event ValidatorWithdrawn(address indexed validator, uint256 amount);
    
    /// @notice Emitted when the validator set changes (for off-chain indexing)
    event ValidatorSetUpdated(uint256 indexed blockNumber);

    /// @notice Register as a validator with a secp256k1 public key
    /// @param pubkeyXY The 64-byte uncompressed public key (x||y coordinates, no 0x04 prefix)
    /// @dev The public key must derive to msg.sender using Ethereum's address derivation
    function register(bytes calldata pubkeyXY) external {
        require(pubkeyXY.length == 64, "Invalid pubkey length");
        
        // Derive address from public key and verify it matches msg.sender
        address derivedAddress = address(uint160(uint256(keccak256(pubkeyXY))));
        require(derivedAddress == msg.sender, "Pubkey does not match sender");
        
        // Add to validator list if not already registered
        if (!validators[msg.sender].exists) {
            validators[msg.sender].exists = true;
            validatorAddresses.push(msg.sender);
        }
        
        // Store/update public key
        validators[msg.sender].pubkeyXY = pubkeyXY;
        
        emit ValidatorRegistered(msg.sender, pubkeyXY);
    }

    /// @notice Bond tokens to increase voting power
    /// @dev 1 wei = 1 stake unit (PoC). Must meet MIN_SELF_DELEGATION to be eligible
    function bond() external payable {
        require(validators[msg.sender].exists, "Validator not registered");
        require(msg.value > 0, "Must bond positive amount");
        
        validators[msg.sender].stake += msg.value;
        
        emit ValidatorBonded(msg.sender, msg.value, validators[msg.sender].stake);
        emit ValidatorSetUpdated(block.number);
    }

    /// @notice Start unbonding process for tokens
    /// @param amount Amount to unbond in wei
    function unbond(uint256 amount) external {
        Validator storage validator = validators[msg.sender];
        require(validator.exists, "Validator not registered");
        require(amount > 0, "Must unbond positive amount");
        require(validator.stake >= amount, "Insufficient stake");
        
        // Reduce stake immediately
        validator.stake -= amount;
        
        // Add to unbonding queue
        uint256 releaseTime = block.timestamp + UNBONDING_PERIOD;
        unbondingQueue[msg.sender].push(UnbondingEntry({
            amount: amount,
            releaseTime: releaseTime
        }));
        
        emit ValidatorUnbonded(msg.sender, amount, releaseTime);
        emit ValidatorSetUpdated(block.number);
    }

    /// @notice Withdraw all available unbonded tokens
    function withdraw() external {
        UnbondingEntry[] storage queue = unbondingQueue[msg.sender];
        uint256 totalWithdrawable = 0;
        uint256 i = 0;
        
        // Process unbonding queue and remove completed entries
        while (i < queue.length) {
            if (queue[i].releaseTime <= block.timestamp) {
                totalWithdrawable += queue[i].amount;
                // Remove entry by swapping with last and popping
                queue[i] = queue[queue.length - 1];
                queue.pop();
            } else {
                i++;
            }
        }
        
        require(totalWithdrawable > 0, "No withdrawable funds");
        
        // Transfer funds
        payable(msg.sender).transfer(totalWithdrawable);
        
        emit ValidatorWithdrawn(msg.sender, totalWithdrawable);
    }

    /// @notice Get the current active validator set
    /// @return pubkeys Array of public keys (64 bytes each)
    /// @return powers Array of voting powers (stake / POWER_REDUCTION)
    /// @return addresses Array of validator addresses
    function getCurrentValidatorSet() 
        external 
        view 
        returns (
            bytes[] memory pubkeys, 
            uint64[] memory powers, 
            address[] memory addresses
        ) 
    {
        return _getValidatorSet();
    }

    /// @notice Get the validator set for the next block (H+1 semantics)
    /// @return pubkeys Array of public keys (64 bytes each)
    /// @return powers Array of voting powers (stake / POWER_REDUCTION)  
    /// @return addresses Array of validator addresses
    /// @dev This is the same as getCurrentValidatorSet in our simple implementation
    /// In a more complex system, this could apply pending changes
    function getNextValidatorSet() 
        external 
        view 
        returns (
            bytes[] memory pubkeys, 
            uint64[] memory powers, 
            address[] memory addresses
        ) 
    {
        return _getValidatorSet();
    }

    /// @notice Internal function to compute the active validator set
    /// @dev Applies Cosmos-style sorting: power descending, then address ascending
    function _getValidatorSet() 
        internal 
        view 
        returns (
            bytes[] memory pubkeys, 
            uint64[] memory powers, 
            address[] memory addresses
        ) 
    {
        // Count eligible validators (meet min self-delegation)
        uint256 eligibleCount = 0;
        for (uint256 i = 0; i < validatorAddresses.length; i++) {
            address validator = validatorAddresses[i];
            if (validators[validator].stake >= MIN_SELF_DELEGATION) {
                eligibleCount++;
            }
        }
        
        if (eligibleCount == 0) {
            return (new bytes[](0), new uint64[](0), new address[](0));
        }
        
        // Collect eligible validators
        address[] memory eligibleValidators = new address[](eligibleCount);
        uint256[] memory eligiblePowers = new uint256[](eligibleCount);
        uint256 index = 0;
        
        for (uint256 i = 0; i < validatorAddresses.length; i++) {
            address validator = validatorAddresses[i];
            uint256 stake = validators[validator].stake;
            if (stake >= MIN_SELF_DELEGATION) {
                eligibleValidators[index] = validator;
                eligiblePowers[index] = stake / POWER_REDUCTION;
                index++;
            }
        }
        
        // Sort by power (descending) then by address (ascending) - Cosmos style
        _sortValidators(eligibleValidators, eligiblePowers);
        
        // Take top MAX_VALIDATORS
        uint256 activeCount = eligibleCount > MAX_VALIDATORS ? MAX_VALIDATORS : eligibleCount;
        
        pubkeys = new bytes[](activeCount);
        powers = new uint64[](activeCount);
        addresses = new address[](activeCount);
        
        for (uint256 i = 0; i < activeCount; i++) {
            address validator = eligibleValidators[i];
            pubkeys[i] = validators[validator].pubkeyXY;
            powers[i] = uint64(eligiblePowers[i]);
            addresses[i] = validator;
        }
    }

    /// @notice Sort validators by power (desc) then address (asc) - Cosmos consensus style
    /// @dev Simple bubble sort for PoC (fine for small validator sets)
    function _sortValidators(address[] memory addrs, uint256[] memory powers) internal pure {
        uint256 n = addrs.length;
        for (uint256 i = 0; i < n - 1; i++) {
            for (uint256 j = 0; j < n - i - 1; j++) {
                bool shouldSwap = false;
                
                // Primary sort: power descending
                if (powers[j] < powers[j + 1]) {
                    shouldSwap = true;
                } else if (powers[j] == powers[j + 1]) {
                    // Secondary sort: address ascending
                    if (addrs[j] > addrs[j + 1]) {
                        shouldSwap = true;
                    }
                }
                
                if (shouldSwap) {
                    // Swap addresses
                    address tempAddr = addrs[j];
                    addrs[j] = addrs[j + 1];
                    addrs[j + 1] = tempAddr;
                    
                    // Swap powers
                    uint256 tempPower = powers[j];
                    powers[j] = powers[j + 1];
                    powers[j + 1] = tempPower;
                }
            }
        }
    }

    /// @notice Get validator info by address
    function getValidator(address validator) 
        external 
        view 
        returns (
            bytes memory pubkeyXY,
            uint256 stake,
            bool exists
        ) 
    {
        Validator storage val = validators[validator];
        return (val.pubkeyXY, val.stake, val.exists);
    }

    /// @notice Get unbonding queue length for a validator
    function getUnbondingQueueLength(address validator) external view returns (uint256) {
        return unbondingQueue[validator].length;
    }

    /// @notice Get specific unbonding entry
    function getUnbondingEntry(address validator, uint256 index) 
        external 
        view 
        returns (uint256 amount, uint256 releaseTime) 
    {
        require(index < unbondingQueue[validator].length, "Index out of bounds");
        UnbondingEntry storage entry = unbondingQueue[validator][index];
        return (entry.amount, entry.releaseTime);
    }

    /// @notice Get total number of registered validators
    function getTotalValidators() external view returns (uint256) {
        return validatorAddresses.length;
    }

    /// @notice Get validator address by index
    function getValidatorByIndex(uint256 index) external view returns (address) {
        require(index < validatorAddresses.length, "Index out of bounds");
        return validatorAddresses[index];
    }
}