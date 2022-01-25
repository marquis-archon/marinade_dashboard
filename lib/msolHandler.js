var mSOLHandler = {};

mSOLHandler._mSOL = {};
// Validators
mSOLHandler.mSOL = function (data, callback) {
  var acceptableMethods = ["post", "get", "put", "delete"];
  if (acceptableMethods.indexOf(data.method) > -1) {
    mSOLHandler._mSOL[data.method](data, callback);
  } else {
    callback(405);
  }
};

// Circulating supply calculations
const { clusterApiUrl, Connection, PublicKey } = require("@solana/web3.js");
let connection = new Connection(clusterApiUrl("mainnet-beta"));
const mSOL_KEY = new PublicKey("mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So");

let MSOL_SUPPLY;

const setData = async () => {
  // Set msol supply
  const mSOLInfo = await connection.getTokenSupply(mSOL_KEY);
  MSOL_SUPPLY = Math.ceil(mSOLInfo.value.uiAmount);
  console.log(MSOL_SUPPLY);
};

setData();
// Set scheduled job to update circulating supply infor
// setInterval(() => {
//   setVaultBalance();
// }, 60 * 60 * 1000);

mSOLHandler._mSOL.get = function (data, callback) {
  result = {};
  if ("supply" in data.queryStringObject) {
    result.mSOLSupply = MSOL_SUPPLY;
    callback(200, result);
  } else {
    result.warning = "Not a valid endpoint";
    result.validEndpoints = ["/mnde?mSOLSupply"];
    callback(400, result);
  }
};

// Export the handlers
module.exports = mSOLHandler;
