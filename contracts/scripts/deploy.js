/**
 * Deploy NodalyncSettlement contract using Hardhat
 *
 * Usage:
 *   npx hardhat run scripts/deploy.js --network hederaTestnet
 *
 * For local testing:
 *   npx hardhat run scripts/deploy.js --network hardhat
 */

const hre = require("hardhat");
const fs = require("fs");

async function main() {
  console.log("=".repeat(60));
  console.log("Nodalync Settlement Contract Deployment");
  console.log("=".repeat(60));

  // Get network info
  const network = hre.network.name;
  console.log(`Network: ${network}`);

  // Get deployer account
  const [deployer] = await hre.ethers.getSigners();
  console.log(`Deployer: ${deployer.address}`);

  // Check balance
  const balance = await hre.ethers.provider.getBalance(deployer.address);
  console.log(`Balance: ${hre.ethers.formatEther(balance)} ETH/HBAR`);

  if (balance === 0n) {
    console.error("ERROR: Deployer has no balance!");
    console.log("For Hedera testnet, get test HBAR from: https://portal.hedera.com/faucet");
    process.exit(1);
  }

  console.log("\nDeploying NodalyncSettlement...");

  // Deploy contract
  const NodalyncSettlement = await hre.ethers.getContractFactory("NodalyncSettlement");

  // Estimate gas
  const deployTx = await NodalyncSettlement.getDeployTransaction();
  const estimatedGas = await hre.ethers.provider.estimateGas(deployTx);
  console.log(`Estimated gas: ${estimatedGas.toString()}`);

  // Deploy
  const settlement = await NodalyncSettlement.deploy();
  await settlement.waitForDeployment();

  const contractAddress = await settlement.getAddress();

  console.log("\n" + "=".repeat(60));
  console.log("DEPLOYMENT SUCCESSFUL!");
  console.log("=".repeat(60));
  console.log(`Contract Address: ${contractAddress}`);

  // Convert to Hedera format if on Hedera network
  if (network.includes("hedera")) {
    console.log(`\nNote: Look up the Contract ID on HashScan:`);
    console.log(`https://hashscan.io/testnet/contract/${contractAddress}`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("Next Steps:");
  console.log("=".repeat(60));
  console.log(`
1. Copy the contract address above

2. Set the environment variable:
   export HEDERA_CONTRACT_ID=${contractAddress}

3. Run integration tests:
   cd ../crates/nodalync-settle
   HEDERA_ACCOUNT_ID=0.0.7703962 \\
   HEDERA_PRIVATE_KEY=0xd21f3bfe69929b1d6e0f37fa9622b96f874a892f7236a7e0e3c8d7b62b422d8b \\
   HEDERA_CONTRACT_ID=${contractAddress} \\
   cargo test --features testnet -- --ignored --nocapture
`);

  // Verify contract is working
  console.log("\nVerifying contract...");
  const owner = await settlement.owner();
  console.log(`Contract owner: ${owner}`);

  const disputePeriod = await settlement.DISPUTE_PERIOD();
  console.log(`Dispute period: ${disputePeriod.toString()} seconds (${Number(disputePeriod) / 3600} hours)`);

  console.log("\nContract verified successfully!");

  // Save deployment info
  const deploymentInfo = {
    network: network,
    contractAddress: contractAddress,
    deployer: deployer.address,
    deployedAt: new Date().toISOString(),
    transactionHash: settlement.deploymentTransaction()?.hash,
  };

  const deploymentPath = `./deployments/${network}.json`;

  // Create deployments directory if it doesn't exist
  if (!fs.existsSync("./deployments")) {
    fs.mkdirSync("./deployments");
  }

  fs.writeFileSync(deploymentPath, JSON.stringify(deploymentInfo, null, 2));
  console.log(`\nDeployment info saved to: ${deploymentPath}`);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error("Deployment failed:", error);
    process.exit(1);
  });
