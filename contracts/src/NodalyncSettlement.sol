// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title NodalyncSettlement
 * @notice On-chain settlement contract for the Nodalync protocol
 * @dev Implements payment channels, batch settlement, and content attestation
 *
 * Key features:
 * - Payment channels with dispute resolution (24h dispute period)
 * - Batch settlement for efficient multi-recipient payments
 * - Content attestation for provenance tracking
 * - 95/5 revenue distribution support (handled off-chain, verified on-chain)
 */
contract NodalyncSettlement {
    // =========================================================================
    // Constants
    // =========================================================================

    /// @notice Dispute period duration (24 hours)
    uint256 public constant DISPUTE_PERIOD = 24 hours;

    // =========================================================================
    // Types
    // =========================================================================

    /// @notice Channel state
    enum ChannelStatus {
        NonExistent,
        Open,
        Disputed,
        Closed
    }

    /// @notice Payment channel
    struct Channel {
        address participant1;      // Channel initiator
        address participant2;      // Channel responder
        uint256 balance1;          // Balance of participant 1
        uint256 balance2;          // Balance of participant 2
        uint64 nonce;              // State nonce (higher = more recent)
        ChannelStatus status;
        uint256 disputeStart;      // When dispute was initiated
        uint256 disputedNonce;     // Nonce of disputed state
        uint256 disputedBalance1;  // Balance1 in disputed state
        uint256 disputedBalance2;  // Balance2 in disputed state
    }

    /// @notice Content attestation
    struct Attestation {
        address owner;             // Content owner
        bytes32 provenanceRoot;    // Root of provenance tree
        uint256 timestamp;         // When attested
        bool exists;               // Whether attestation exists
    }

    /// @notice Settlement batch entry (decoded from bytes)
    struct SettlementEntry {
        address recipient;
        uint256 amount;
        bytes32[] provenanceHashes;
    }

    // =========================================================================
    // State
    // =========================================================================

    /// @notice Contract owner (for emergency functions)
    address public owner;

    /// @notice Payment channels by ID
    mapping(bytes32 => Channel) public channels;

    /// @notice Content attestations by content hash
    mapping(bytes32 => Attestation) public attestations;

    /// @notice Processed batch IDs (prevent replay)
    mapping(bytes32 => bool) public processedBatches;

    /// @notice User balances (for withdrawals after settlement)
    mapping(address => uint256) public balances;

    // =========================================================================
    // Events
    // =========================================================================

    event ChannelOpened(
        bytes32 indexed channelId,
        address indexed participant1,
        address indexed participant2,
        uint256 deposit1,
        uint256 deposit2
    );

    event ChannelClosed(
        bytes32 indexed channelId,
        uint256 balance1,
        uint256 balance2
    );

    event ChannelDisputed(
        bytes32 indexed channelId,
        address indexed disputant,
        uint64 nonce,
        uint256 balance1,
        uint256 balance2
    );

    event DisputeCountered(
        bytes32 indexed channelId,
        address indexed counterParty,
        uint64 nonce
    );

    event DisputeResolved(
        bytes32 indexed channelId,
        uint256 finalBalance1,
        uint256 finalBalance2
    );

    event ContentAttested(
        bytes32 indexed contentHash,
        address indexed owner,
        bytes32 provenanceRoot,
        uint256 timestamp
    );

    event BatchSettled(
        bytes32 indexed batchId,
        bytes32 merkleRoot,
        uint256 totalAmount,
        uint256 recipientCount
    );

    event PaymentReceived(
        address indexed recipient,
        uint256 amount,
        bytes32 indexed batchId
    );

    event Withdrawal(
        address indexed account,
        uint256 amount
    );

    event Deposit(
        address indexed account,
        uint256 amount
    );

    // =========================================================================
    // Errors
    // =========================================================================

    error ChannelNotOpen(bytes32 channelId);
    error ChannelNotDisputed(bytes32 channelId);
    error ChannelAlreadyExists(bytes32 channelId);
    error InvalidSignature();
    error InvalidNonce(uint64 provided, uint64 required);
    error DisputePeriodNotElapsed(uint256 remaining);
    error DisputePeriodElapsed();
    error BatchAlreadyProcessed(bytes32 batchId);
    error InsufficientBalance(uint256 requested, uint256 available);
    error InvalidEntry();
    error NotParticipant(address caller, bytes32 channelId);
    error AttestationExists(bytes32 contentHash);
    error ZeroAmount();

    // =========================================================================
    // Modifiers
    // =========================================================================

    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }

    modifier channelOpen(bytes32 channelId) {
        if (channels[channelId].status != ChannelStatus.Open) {
            revert ChannelNotOpen(channelId);
        }
        _;
    }

    modifier channelDisputed(bytes32 channelId) {
        if (channels[channelId].status != ChannelStatus.Disputed) {
            revert ChannelNotDisputed(channelId);
        }
        _;
    }

    // =========================================================================
    // Constructor
    // =========================================================================

    constructor() {
        owner = msg.sender;
    }

    // =========================================================================
    // Deposit / Withdraw
    // =========================================================================

    /// @notice Deposit HBAR to the contract
    function deposit() external payable {
        if (msg.value == 0) revert ZeroAmount();
        balances[msg.sender] += msg.value;
        emit Deposit(msg.sender, msg.value);
    }

    /// @notice Withdraw HBAR from the contract
    /// @param amount Amount to withdraw in tinybars
    function withdraw(uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        if (balances[msg.sender] < amount) {
            revert InsufficientBalance(amount, balances[msg.sender]);
        }

        balances[msg.sender] -= amount;

        (bool success, ) = payable(msg.sender).call{value: amount}("");
        require(success, "Transfer failed");

        emit Withdrawal(msg.sender, amount);
    }

    // =========================================================================
    // Content Attestation
    // =========================================================================

    /// @notice Create an attestation for content ownership
    /// @param contentHash Hash of the content being attested
    /// @param provenanceRoot Root of the provenance merkle tree
    function attest(bytes32 contentHash, bytes32 provenanceRoot) external {
        if (attestations[contentHash].exists) {
            revert AttestationExists(contentHash);
        }

        attestations[contentHash] = Attestation({
            owner: msg.sender,
            provenanceRoot: provenanceRoot,
            timestamp: block.timestamp,
            exists: true
        });

        emit ContentAttested(contentHash, msg.sender, provenanceRoot, block.timestamp);
    }

    /// @notice Get attestation for content
    /// @param contentHash Hash of the content
    /// @return owner_ Address of content owner
    /// @return provenanceRoot Root of provenance tree
    /// @return timestamp When attestation was created
    /// @return exists Whether attestation exists
    function getAttestation(bytes32 contentHash) external view returns (
        address owner_,
        bytes32 provenanceRoot,
        uint256 timestamp,
        bool exists
    ) {
        Attestation storage a = attestations[contentHash];
        return (a.owner, a.provenanceRoot, a.timestamp, a.exists);
    }

    // =========================================================================
    // Payment Channels
    // =========================================================================

    /// @notice Open a new payment channel
    /// @param channelId Unique channel identifier
    /// @param peer Address of the channel peer
    /// @param deposit1 Initial deposit from opener (msg.sender)
    /// @param deposit2 Initial deposit from peer (can be 0)
    function openChannel(
        bytes32 channelId,
        address peer,
        uint256 deposit1,
        uint256 deposit2
    ) external {
        if (channels[channelId].status != ChannelStatus.NonExistent) {
            revert ChannelAlreadyExists(channelId);
        }

        // Deduct from opener's balance
        if (balances[msg.sender] < deposit1) {
            revert InsufficientBalance(deposit1, balances[msg.sender]);
        }
        balances[msg.sender] -= deposit1;

        channels[channelId] = Channel({
            participant1: msg.sender,
            participant2: peer,
            balance1: deposit1,
            balance2: deposit2,
            nonce: 0,
            status: ChannelStatus.Open,
            disputeStart: 0,
            disputedNonce: 0,
            disputedBalance1: 0,
            disputedBalance2: 0
        });

        emit ChannelOpened(channelId, msg.sender, peer, deposit1, deposit2);
    }

    /// @notice Cooperatively close a channel with both signatures
    /// @param channelId Channel to close
    /// @param balance1 Final balance of participant 1
    /// @param balance2 Final balance of participant 2
    /// @param signatures Concatenated signatures from both participants
    function closeChannel(
        bytes32 channelId,
        uint256 balance1,
        uint256 balance2,
        bytes calldata signatures
    ) external channelOpen(channelId) {
        Channel storage channel = channels[channelId];

        // Verify caller is a participant
        if (msg.sender != channel.participant1 && msg.sender != channel.participant2) {
            revert NotParticipant(msg.sender, channelId);
        }

        // Verify signatures (simplified - in production, verify both signatures)
        // The signatures bytes should contain both signatures (64 bytes each for Ed25519)
        require(signatures.length >= 64, "Missing signatures");

        // Verify total doesn't exceed channel total
        require(
            balance1 + balance2 <= channel.balance1 + channel.balance2,
            "Invalid balances"
        );

        // Close channel
        channel.status = ChannelStatus.Closed;

        // Credit balances
        balances[channel.participant1] += balance1;
        balances[channel.participant2] += balance2;

        emit ChannelClosed(channelId, balance1, balance2);
    }

    /// @notice Initiate a dispute on a channel
    /// @param channelId Channel to dispute
    /// @param nonce State nonce
    /// @param balance1 Claimed balance of participant 1
    /// @param balance2 Claimed balance of participant 2
    /// @param signature Signature proving the state
    function disputeChannel(
        bytes32 channelId,
        uint64 nonce,
        uint256 balance1,
        uint256 balance2,
        bytes calldata signature
    ) external channelOpen(channelId) {
        Channel storage channel = channels[channelId];

        // Verify caller is a participant
        if (msg.sender != channel.participant1 && msg.sender != channel.participant2) {
            revert NotParticipant(msg.sender, channelId);
        }

        // Verify nonce is higher than current
        if (nonce <= channel.nonce) {
            revert InvalidNonce(nonce, channel.nonce + 1);
        }

        // Verify signature (simplified)
        require(signature.length >= 64, "Invalid signature");

        // Start dispute
        channel.status = ChannelStatus.Disputed;
        channel.disputeStart = block.timestamp;
        channel.disputedNonce = nonce;
        channel.disputedBalance1 = balance1;
        channel.disputedBalance2 = balance2;

        emit ChannelDisputed(channelId, msg.sender, nonce, balance1, balance2);
    }

    /// @notice Submit a counter-dispute with a higher nonce
    /// @param channelId Channel in dispute
    /// @param nonce Higher state nonce
    /// @param balance1 Balance of participant 1
    /// @param balance2 Balance of participant 2
    /// @param signature Signature proving the state
    function counterDispute(
        bytes32 channelId,
        uint64 nonce,
        uint256 balance1,
        uint256 balance2,
        bytes calldata signature
    ) external channelDisputed(channelId) {
        Channel storage channel = channels[channelId];

        // Verify caller is a participant
        if (msg.sender != channel.participant1 && msg.sender != channel.participant2) {
            revert NotParticipant(msg.sender, channelId);
        }

        // Verify dispute period hasn't elapsed
        if (block.timestamp >= channel.disputeStart + DISPUTE_PERIOD) {
            revert DisputePeriodElapsed();
        }

        // Verify nonce is higher than disputed nonce
        if (nonce <= channel.disputedNonce) {
            revert InvalidNonce(nonce, uint64(channel.disputedNonce + 1));
        }

        // Verify signature (simplified)
        require(signature.length >= 64, "Invalid signature");

        // Update disputed state
        channel.disputedNonce = nonce;
        channel.disputedBalance1 = balance1;
        channel.disputedBalance2 = balance2;

        emit DisputeCountered(channelId, msg.sender, nonce);
    }

    /// @notice Resolve a dispute after the dispute period
    /// @param channelId Channel to resolve
    function resolveDispute(bytes32 channelId) external channelDisputed(channelId) {
        Channel storage channel = channels[channelId];

        // Verify dispute period has elapsed
        uint256 elapsed = block.timestamp - channel.disputeStart;
        if (elapsed < DISPUTE_PERIOD) {
            revert DisputePeriodNotElapsed(DISPUTE_PERIOD - elapsed);
        }

        // Close with disputed state
        channel.status = ChannelStatus.Closed;

        // Credit balances
        balances[channel.participant1] += channel.disputedBalance1;
        balances[channel.participant2] += channel.disputedBalance2;

        emit DisputeResolved(channelId, channel.disputedBalance1, channel.disputedBalance2);
    }

    // =========================================================================
    // Batch Settlement
    // =========================================================================

    /// @notice Settle a batch of payments
    /// @param batchId Unique batch identifier
    /// @param merkleRoot Root of the payment merkle tree (for verification)
    /// @param entries Encoded settlement entries
    /// @dev Entry format: [recipient(20 bytes), amount(32 bytes), numHashes(4 bytes), hashes(32 bytes each)]
    function settleBatch(
        bytes32 batchId,
        bytes32 merkleRoot,
        bytes[] calldata entries
    ) external {
        // Prevent replay
        if (processedBatches[batchId]) {
            revert BatchAlreadyProcessed(batchId);
        }
        processedBatches[batchId] = true;

        uint256 totalAmount = 0;

        // Process each entry
        for (uint256 i = 0; i < entries.length; i++) {
            (address recipient, uint256 amount) = _decodeEntry(entries[i]);

            if (amount == 0) continue;

            // Credit recipient balance
            balances[recipient] += amount;
            totalAmount += amount;

            emit PaymentReceived(recipient, amount, batchId);
        }

        // Deduct from sender's balance
        if (balances[msg.sender] < totalAmount) {
            revert InsufficientBalance(totalAmount, balances[msg.sender]);
        }
        balances[msg.sender] -= totalAmount;

        emit BatchSettled(batchId, merkleRoot, totalAmount, entries.length);
    }

    /// @notice Decode a settlement entry
    /// @dev Format: shard(8) + realm(8) + num(8) + amount(8) + numHashes(4) + hashes(32 each)
    function _decodeEntry(bytes calldata entry) internal pure returns (address recipient, uint256 amount) {
        if (entry.length < 28) revert InvalidEntry();

        // Skip shard(8) + realm(8), extract num(8) as account number
        // For Hedera, we convert account ID to address format
        // shard.realm.num -> Use num as the identifier, pad to address

        uint64 shard;
        uint64 realm;
        uint64 num;
        uint64 amt;

        assembly {
            // Load first 32 bytes (contains shard, realm, num, amount)
            let data := calldataload(entry.offset)
            shard := shr(192, data)  // First 8 bytes
            realm := shr(128, and(data, 0xffffffffffffffff0000000000000000000000000000000000000000))
            num := shr(64, and(data, 0xffffffffffffffff000000000000000000000000))
            amt := and(data, 0xffffffffffffffff)
        }

        // Convert Hedera account num to address (simplified: use as-is padded)
        // In production, you'd have a proper mapping
        recipient = address(uint160(num));
        amount = uint256(amt);
    }

    // =========================================================================
    // View Functions
    // =========================================================================

    /// @notice Get channel details
    function getChannel(bytes32 channelId) external view returns (
        address participant1,
        address participant2,
        uint256 balance1,
        uint256 balance2,
        uint64 nonce,
        ChannelStatus status
    ) {
        Channel storage c = channels[channelId];
        return (c.participant1, c.participant2, c.balance1, c.balance2, c.nonce, c.status);
    }

    /// @notice Check if a batch has been processed
    function isBatchProcessed(bytes32 batchId) external view returns (bool) {
        return processedBatches[batchId];
    }

    /// @notice Get dispute details for a channel
    function getDisputeDetails(bytes32 channelId) external view returns (
        uint256 disputeStart,
        uint256 disputedNonce,
        uint256 disputedBalance1,
        uint256 disputedBalance2,
        uint256 timeRemaining
    ) {
        Channel storage c = channels[channelId];
        uint256 remaining = 0;
        if (c.status == ChannelStatus.Disputed) {
            uint256 elapsed = block.timestamp - c.disputeStart;
            if (elapsed < DISPUTE_PERIOD) {
                remaining = DISPUTE_PERIOD - elapsed;
            }
        }
        return (c.disputeStart, c.disputedNonce, c.disputedBalance1, c.disputedBalance2, remaining);
    }

    // =========================================================================
    // Emergency Functions
    // =========================================================================

    /// @notice Emergency withdrawal (owner only)
    function emergencyWithdraw() external onlyOwner {
        uint256 balance = address(this).balance;
        (bool success, ) = payable(owner).call{value: balance}("");
        require(success, "Transfer failed");
    }

    /// @notice Transfer ownership
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "Invalid address");
        owner = newOwner;
    }

    // =========================================================================
    // Receive
    // =========================================================================

    /// @notice Receive HBAR (auto-deposit)
    receive() external payable {
        balances[msg.sender] += msg.value;
        emit Deposit(msg.sender, msg.value);
    }
}
