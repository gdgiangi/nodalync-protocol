require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

/** @type import('hardhat/config').HardhatUserConfig */
module.exports = {
  solidity: {
    version: "0.8.20",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200,
      },
    },
  },
  networks: {
    // Local Hardhat network for testing
    hardhat: {
      chainId: 31337,
    },
    // Hedera Testnet (via JSON-RPC relay)
    hederaTestnet: {
      url: process.env.HEDERA_RPC_URL || "https://testnet.hashio.io/api",
      accounts: process.env.HEDERA_PRIVATE_KEY
        ? [process.env.HEDERA_PRIVATE_KEY.replace("0x", "")]
        : [],
      chainId: 296,
      timeout: 120000, // Hedera can be slow
    },
    // Hedera Mainnet (via JSON-RPC relay)
    hederaMainnet: {
      url: process.env.HEDERA_MAINNET_RPC_URL || "https://mainnet.hashio.io/api",
      accounts: process.env.HEDERA_MAINNET_PRIVATE_KEY
        ? [process.env.HEDERA_MAINNET_PRIVATE_KEY.replace("0x", "")]
        : [],
      chainId: 295,
      timeout: 120000,
    },
  },
  paths: {
    sources: "./src",
    tests: "./test",
    cache: "./cache",
    artifacts: "./artifacts",
  },
};
