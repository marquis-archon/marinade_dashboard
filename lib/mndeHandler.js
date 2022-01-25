var mndeHandler = {};

mndeHandler._mnde = {};
// Validators
mndeHandler.mnde = function (data, callback) {
  var acceptableMethods = ["post", "get", "put", "delete"];
  if (acceptableMethods.indexOf(data.method) > -1) {
    mndeHandler._mnde[data.method](data, callback);
  } else {
    callback(405);
  }
};

// Circulating supply calculations
const { clusterApiUrl, Connection, PublicKey } = require("@solana/web3.js");
let connection = new Connection(clusterApiUrl("mainnet-beta"));
const VAULT_KEY = new PublicKey("GR1LBT4cU89cJWE74CP6BsJTf2kriQ9TX59tbDsfxgSi");

const MNDE_TOTAL_SUPPLY = 1_000_000_000;
let VAULT_BALANCE;
let MNDE_CIRCULATING_SUPPLY;

const setData = async () => {
  // Set vault balance
  const tokenBalanceInfo = await connection.getTokenAccountBalance(VAULT_KEY);
  VAULT_BALANCE = tokenBalanceInfo.value.uiAmount;
  MNDE_CIRCULATING_SUPPLY = Math.ceil(MNDE_TOTAL_SUPPLY - VAULT_BALANCE);
};

setData();
// Set scheduled job to update circulating supply infor
// setInterval(() => {
//   setVaultBalance();
// }, 60 * 60 * 1000);

mndeHandler._mnde.get = function (data, callback) {
  result = {};
  if ("total_supply" in data.queryStringObject) {
    result.mnde_total_supply = MNDE_TOTAL_SUPPLY;
    callback(200, result);
  } else if ("circulating_supply" in data.queryStringObject) {
    result.mnde_circulating_supply = MNDE_CIRCULATING_SUPPLY;
    callback(200, result);
  } else {
    result.warning = "Not a valid endpoint";
    result.validEndpoints = ["/mnde?circulating_supply", "/mnde?total_supply"];
    callback(400, result);
  }
};

// Export the handlers
module.exports = mndeHandler;
