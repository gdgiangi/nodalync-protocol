/**
 * Deploy NodalyncSettlement contract using Hedera SDK directly
 *
 * This is an alternative deployment method that uses the Hedera SDK
 * instead of the JSON-RPC relay. Use this if the Hardhat deployment
 * has issues with Hedera.
 *
 * Usage:
 *   node scripts/deploy-hedera-sdk.js
 *
 * Environment variables:
 *   HEDERA_ACCOUNT_ID - Your Hedera account ID (e.g., 0.0.7703962)
 *   HEDERA_PRIVATE_KEY - Your private key (hex encoded)
 */

const {
  Client,
  ContractCreateFlow,
  PrivateKey,
  AccountId,
  Hbar,
  ContractCallQuery,
} = require("@hashgraph/sdk");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

require("dotenv").config();

async function main() {
  console.log("=".repeat(60));
  console.log("Nodalync Settlement Contract Deployment (Hedera SDK)");
  console.log("=".repeat(60));

  // Get credentials from environment
  const { accountIdStr, privateKeyStr } = readCredentials();
  console.log(`Account ID: ${accountIdStr}`);

  // Parse credentials
  const accountId = AccountId.fromString(accountIdStr);
  const privateKey = PrivateKey.fromStringED25519(
    privateKeyStr.replace("0x", "")
  );

  console.log(`Public Key: ${privateKey.publicKey.toString()}`);

  const client = createTestnetClient(accountId, privateKey);

  // Read compiled bytecode
  const bytecode = loadBytecode();

  // Deploy contract
  const { contractId, transactionId } = await deployContract(
    client,
    bytecode,
    privateKey
  );

  // Get EVM address for the contract
  const evmAddress = contractId.toSolidityAddress();
  console.log(`EVM Address: 0x${evmAddress}`);

  // Verify contract is working
  await verifyContract(client, contractId);

  printNextSteps(contractId, accountIdStr, privateKeyStr);

  // Save deployment info
  saveDeployment({
    network: "hedera-testnet",
    contractId: contractId.toString(),
    evmAddress: `0x${evmAddress}`,
    deployer: accountIdStr,
    deployedAt: new Date().toISOString(),
    transactionId,
  });

  client.close();
}

/** Read the Hedera credentials from the environment, exiting if either is missing. */
function readCredentials() {
  const accountIdStr = process.env.HEDERA_ACCOUNT_ID;
  const privateKeyStr = process.env.HEDERA_PRIVATE_KEY;

  if (!accountIdStr || !privateKeyStr) {
    console.error("ERROR: Missing environment variables");
    console.log("Required:");
    console.log("  HEDERA_ACCOUNT_ID=0.0.XXXXXXX");
    console.log("  HEDERA_PRIVATE_KEY=0x...");
    process.exit(1);
  }
  return { accountIdStr, privateKeyStr };
}

/** Create a testnet client with the operator and fee limits set. */
function createTestnetClient(accountId, privateKey) {
  const client = Client.forTestnet();
  client.setOperator(accountId, privateKey);
  client.setDefaultMaxTransactionFee(new Hbar(100));
  client.setMaxQueryPayment(new Hbar(10));

  console.log("Connected to Hedera Testnet");
  return client;
}

/** The compiled contract's bytecode, compiling with Hardhat first if needed. Exits if it is empty. */
function loadBytecode() {
  console.log("\nChecking for compiled contract...");

  const artifactPath = path.join(
    __dirname,
    "..",
    "artifacts",
    "src",
    "NodalyncSettlement.sol",
    "NodalyncSettlement.json"
  );

  if (!fs.existsSync(artifactPath)) {
    console.log("Contract not compiled. Compiling with Hardhat...");
    execSync("npx hardhat compile", {
      cwd: path.join(__dirname, ".."),
      stdio: "inherit",
    });
  }

  const artifact = JSON.parse(fs.readFileSync(artifactPath, "utf8"));
  const bytecode = artifact.bytecode;

  if (!bytecode || bytecode === "0x") {
    console.error("ERROR: Contract bytecode is empty. Compilation may have failed.");
    process.exit(1);
  }

  console.log(`Bytecode size: ${(bytecode.length - 2) / 2} bytes`);
  return bytecode;
}

/** Deploy the contract and return its ID and the transaction ID. */
async function deployContract(client, bytecode, privateKey) {
  console.log("\nDeploying NodalyncSettlement contract...");

  const contractTx = new ContractCreateFlow()
    .setBytecode(bytecode)
    .setGas(2000000) // Hedera requires explicit gas limit
    .setAdminKey(privateKey.publicKey);

  console.log("Submitting transaction...");

  const contractResponse = await contractTx.execute(client);
  const contractReceipt = await contractResponse.getReceipt(client);
  const contractId = contractReceipt.contractId;

  console.log("\n" + "=".repeat(60));
  console.log("DEPLOYMENT SUCCESSFUL!");
  console.log("=".repeat(60));
  console.log(`Contract ID: ${contractId.toString()}`);
  console.log(`Transaction ID: ${contractResponse.transactionId.toString()}`);

  return {
    contractId,
    transactionId: contractResponse.transactionId.toString(),
  };
}

/** Query DISPUTE_PERIOD to check the contract responds; a failure is only noted. */
async function verifyContract(client, contractId) {
  console.log("\nVerifying contract...");

  try {
    const disputePeriodQuery = new ContractCallQuery()
      .setContractId(contractId)
      .setGas(100000)
      .setFunction("DISPUTE_PERIOD");

    const disputePeriodResult = await disputePeriodQuery.execute(client);
    const disputePeriod = disputePeriodResult.getUint256(0);
    console.log(`Dispute period: ${disputePeriod.toString()} seconds (${Number(disputePeriod) / 3600} hours)`);
  } catch (e) {
    console.log("Note: Could not query DISPUTE_PERIOD (this is OK, contract is deployed)");
  }
}

function printNextSteps(contractId, accountIdStr, privateKeyStr) {
  console.log("\n" + "=".repeat(60));
  console.log("Next Steps:");
  console.log("=".repeat(60));
  console.log(`
1. Your contract ID is: ${contractId.toString()}

2. View on HashScan:
   https://hashscan.io/testnet/contract/${contractId.toString()}

3. Set environment variable for Rust tests:
   export HEDERA_CONTRACT_ID=${contractId.toString()}

4. Run integration tests:
   cd ../crates/nodalync-settle
   HEDERA_ACCOUNT_ID=${accountIdStr} \\
   HEDERA_PRIVATE_KEY=${privateKeyStr} \\
   HEDERA_CONTRACT_ID=${contractId.toString()} \\
   cargo test --features testnet -- --ignored --nocapture

5. Update your .env file:
   echo "HEDERA_CONTRACT_ID=${contractId.toString()}" >> .env
`);
}

/** Write the deployment info to ./deployments/hedera-testnet-sdk.json. */
function saveDeployment(deploymentInfo) {
  // Create deployments directory if it doesn't exist
  if (!fs.existsSync("./deployments")) {
    fs.mkdirSync("./deployments");
  }

  const deploymentPath = "./deployments/hedera-testnet-sdk.json";
  fs.writeFileSync(deploymentPath, JSON.stringify(deploymentInfo, null, 2));
  console.log(`Deployment info saved to: ${deploymentPath}`);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error("Deployment failed:", error);
    process.exit(1);
  });
