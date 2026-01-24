/**
 * NodalyncSettlement Contract Tests
 *
 * Run with: npx hardhat test
 */

const { expect } = require("chai");
const hre = require("hardhat");

const { ethers } = hre;

describe("NodalyncSettlement", function () {
  let settlement;
  let owner;
  let user1;
  let user2;

  const DISPUTE_PERIOD = 24 * 60 * 60; // 24 hours in seconds

  beforeEach(async function () {
    [owner, user1, user2] = await ethers.getSigners();

    const NodalyncSettlement = await ethers.getContractFactory("NodalyncSettlement");
    settlement = await NodalyncSettlement.deploy();
    await settlement.waitForDeployment();
  });

  describe("Deployment", function () {
    it("Should set the correct owner", async function () {
      expect(await settlement.owner()).to.equal(owner.address);
    });

    it("Should have correct dispute period", async function () {
      expect(await settlement.DISPUTE_PERIOD()).to.equal(DISPUTE_PERIOD);
    });
  });

  describe("Deposits and Withdrawals", function () {
    it("Should accept deposits", async function () {
      const depositAmount = ethers.parseEther("1.0");

      await expect(settlement.deposit({ value: depositAmount }))
        .to.emit(settlement, "Deposit")
        .withArgs(owner.address, depositAmount);

      expect(await settlement.balances(owner.address)).to.equal(depositAmount);
    });

    it("Should allow withdrawals", async function () {
      const depositAmount = ethers.parseEther("1.0");
      const withdrawAmount = ethers.parseEther("0.5");

      // Deposit first
      await settlement.deposit({ value: depositAmount });

      // Withdraw
      await expect(settlement.withdraw(withdrawAmount))
        .to.emit(settlement, "Withdrawal")
        .withArgs(owner.address, withdrawAmount);

      expect(await settlement.balances(owner.address)).to.equal(
        depositAmount - withdrawAmount
      );
    });

    it("Should reject withdrawal with insufficient balance", async function () {
      const withdrawAmount = ethers.parseEther("1.0");

      await expect(settlement.withdraw(withdrawAmount)).to.be.revertedWithCustomError(
        settlement,
        "InsufficientBalance"
      );
    });

    it("Should accept direct HBAR transfers", async function () {
      const amount = ethers.parseEther("1.0");

      await owner.sendTransaction({
        to: await settlement.getAddress(),
        value: amount,
      });

      expect(await settlement.balances(owner.address)).to.equal(amount);
    });
  });

  describe("Content Attestation", function () {
    it("Should create attestation", async function () {
      const contentHash = ethers.keccak256(ethers.toUtf8Bytes("test content"));
      const provenanceRoot = ethers.keccak256(ethers.toUtf8Bytes("provenance"));

      await expect(settlement.attest(contentHash, provenanceRoot))
        .to.emit(settlement, "ContentAttested");

      const attestation = await settlement.getAttestation(contentHash);
      expect(attestation.owner_).to.equal(owner.address);
      expect(attestation.provenanceRoot).to.equal(provenanceRoot);
      expect(attestation.exists).to.be.true;
    });

    it("Should reject duplicate attestation", async function () {
      const contentHash = ethers.keccak256(ethers.toUtf8Bytes("test content"));
      const provenanceRoot = ethers.keccak256(ethers.toUtf8Bytes("provenance"));

      await settlement.attest(contentHash, provenanceRoot);

      await expect(
        settlement.attest(contentHash, provenanceRoot)
      ).to.be.revertedWithCustomError(settlement, "AttestationExists");
    });
  });

  describe("Payment Channels", function () {
    const channelId = ethers.keccak256(ethers.toUtf8Bytes("channel1"));
    const deposit1 = ethers.parseEther("10.0");
    const deposit2 = ethers.parseEther("0");

    beforeEach(async function () {
      // Deposit funds for channel
      await settlement.deposit({ value: deposit1 });
    });

    it("Should open a channel", async function () {
      await expect(settlement.openChannel(channelId, user1.address, deposit1, deposit2))
        .to.emit(settlement, "ChannelOpened")
        .withArgs(channelId, owner.address, user1.address, deposit1, deposit2);

      const channel = await settlement.getChannel(channelId);
      expect(channel.participant1).to.equal(owner.address);
      expect(channel.participant2).to.equal(user1.address);
      expect(channel.balance1).to.equal(deposit1);
      expect(channel.status).to.equal(1n); // Open
    });

    it("Should reject duplicate channel", async function () {
      await settlement.openChannel(channelId, user1.address, deposit1, deposit2);

      await expect(
        settlement.openChannel(channelId, user1.address, deposit1, deposit2)
      ).to.be.revertedWithCustomError(settlement, "ChannelAlreadyExists");
    });

    it("Should close channel cooperatively", async function () {
      await settlement.openChannel(channelId, user1.address, deposit1, deposit2);

      const finalBalance1 = ethers.parseEther("6.0");
      const finalBalance2 = ethers.parseEther("4.0");
      const dummySignatures = ethers.hexlify(ethers.randomBytes(128)); // 2 signatures

      await expect(
        settlement.closeChannel(channelId, finalBalance1, finalBalance2, dummySignatures)
      )
        .to.emit(settlement, "ChannelClosed")
        .withArgs(channelId, finalBalance1, finalBalance2);

      // Check balances were credited
      expect(await settlement.balances(owner.address)).to.equal(finalBalance1);
      expect(await settlement.balances(user1.address)).to.equal(finalBalance2);
    });
  });

  describe("Channel Disputes", function () {
    const channelId = ethers.keccak256(ethers.toUtf8Bytes("dispute-channel"));
    const deposit1 = ethers.parseEther("10.0");

    beforeEach(async function () {
      await settlement.deposit({ value: deposit1 });
      await settlement.openChannel(channelId, user1.address, deposit1, 0);
    });

    it("Should initiate dispute", async function () {
      const nonce = 5n;
      const balance1 = ethers.parseEther("7.0");
      const balance2 = ethers.parseEther("3.0");
      const dummySignature = ethers.hexlify(ethers.randomBytes(64));

      await expect(
        settlement.disputeChannel(channelId, nonce, balance1, balance2, dummySignature)
      )
        .to.emit(settlement, "ChannelDisputed")
        .withArgs(channelId, owner.address, nonce, balance1, balance2);

      const channel = await settlement.getChannel(channelId);
      expect(channel.status).to.equal(2n); // Disputed
    });

    it("Should allow counter-dispute with higher nonce", async function () {
      const nonce1 = 5n;
      const nonce2 = 10n;
      const balance1 = ethers.parseEther("7.0");
      const balance2 = ethers.parseEther("3.0");
      const dummySignature = ethers.hexlify(ethers.randomBytes(64));

      // Initial dispute
      await settlement.disputeChannel(channelId, nonce1, balance1, balance2, dummySignature);

      // Counter-dispute with higher nonce
      await expect(
        settlement.counterDispute(channelId, nonce2, balance2, balance1, dummySignature)
      )
        .to.emit(settlement, "DisputeCountered")
        .withArgs(channelId, owner.address, nonce2);
    });

    it("Should resolve dispute after period", async function () {
      const nonce = 5n;
      const balance1 = ethers.parseEther("7.0");
      const balance2 = ethers.parseEther("3.0");
      const dummySignature = ethers.hexlify(ethers.randomBytes(64));

      await settlement.disputeChannel(channelId, nonce, balance1, balance2, dummySignature);

      // Fast forward time
      await ethers.provider.send("evm_increaseTime", [DISPUTE_PERIOD + 1]);
      await ethers.provider.send("evm_mine");

      await expect(settlement.resolveDispute(channelId))
        .to.emit(settlement, "DisputeResolved")
        .withArgs(channelId, balance1, balance2);

      // Balances should be credited
      expect(await settlement.balances(owner.address)).to.equal(balance1);
      expect(await settlement.balances(user1.address)).to.equal(balance2);
    });

    it("Should reject early resolution", async function () {
      const nonce = 5n;
      const balance1 = ethers.parseEther("7.0");
      const balance2 = ethers.parseEther("3.0");
      const dummySignature = ethers.hexlify(ethers.randomBytes(64));

      await settlement.disputeChannel(channelId, nonce, balance1, balance2, dummySignature);

      await expect(settlement.resolveDispute(channelId)).to.be.revertedWithCustomError(
        settlement,
        "DisputePeriodNotElapsed"
      );
    });
  });

  describe("Batch Settlement", function () {
    it("Should settle batch of payments", async function () {
      const batchId = ethers.keccak256(ethers.toUtf8Bytes("batch1"));
      const merkleRoot = ethers.keccak256(ethers.toUtf8Bytes("merkle"));

      // Deposit funds
      await settlement.deposit({ value: ethers.parseEther("100.0") });

      // Create encoded entries
      const entry1 = encodeSettlementEntry(0, 0, BigInt(user1.address), ethers.parseEther("10.0"), []);
      const entry2 = encodeSettlementEntry(0, 0, BigInt(user2.address), ethers.parseEther("20.0"), []);

      await expect(settlement.settleBatch(batchId, merkleRoot, [entry1, entry2]))
        .to.emit(settlement, "BatchSettled");

      // Check batch is marked as processed
      expect(await settlement.isBatchProcessed(batchId)).to.be.true;
    });

    it("Should reject duplicate batch", async function () {
      const batchId = ethers.keccak256(ethers.toUtf8Bytes("batch1"));
      const merkleRoot = ethers.keccak256(ethers.toUtf8Bytes("merkle"));

      await settlement.deposit({ value: ethers.parseEther("100.0") });

      const entry = encodeSettlementEntry(0, 0, BigInt(user1.address), ethers.parseEther("10.0"), []);

      await settlement.settleBatch(batchId, merkleRoot, [entry]);

      await expect(
        settlement.settleBatch(batchId, merkleRoot, [entry])
      ).to.be.revertedWithCustomError(settlement, "BatchAlreadyProcessed");
    });
  });
});

// Helper function
function encodeSettlementEntry(shard, realm, num, amount, hashes) {
  // For testing, use a simple numeric ID instead of full address
  // In production, Hedera account numbers would be used
  const shardBn = BigInt(shard);
  const realmBn = BigInt(realm);
  // Convert address to a number by taking the last 8 bytes
  const numBn = typeof num === 'bigint' ? num % (2n ** 64n) : BigInt(num);
  const amountBn = BigInt(amount);

  // Pack as 8-byte big-endian values
  const buffer = new Uint8Array(32);
  const view = new DataView(buffer.buffer);

  view.setBigUint64(0, shardBn, false);
  view.setBigUint64(8, realmBn, false);
  view.setBigUint64(16, numBn, false);
  view.setBigUint64(24, amountBn, false);

  return ethers.hexlify(buffer);
}
