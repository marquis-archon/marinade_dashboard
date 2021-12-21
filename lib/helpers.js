/*
 * Helpers for various tasks
 *
 */

// Dependencies
var config = require('./config');
var crypto = require('crypto');

// Container for all the helpers
var helpers = {};

// Parse a JSON string to an object in all cases, without throwing
helpers.parseJsonToObject = function(str){
  try{
    var obj = JSON.parse(str);
    return obj;
  } catch(e){
    return {};
  }
};

// Create a SHA256 hash
helpers.hash = function(str){
  if(typeof(str) == 'string' && str.length > 0){
    var hash = crypto.createHmac('sha256', config.hashingSecret).update(str).digest('hex');
    return hash;
  } else {
    return false;
  }
};

// Create a string of random alphanumeric characters, of a given length
helpers.createRandomString = function(strLength){
  strLength = typeof(strLength) == 'number' && strLength > 0 ? strLength : false;
  if(strLength){
    // Define all the possible characters that could go into a string
    var possibleCharacters = 'abcdefghijklmnopqrstuvwxyz0123456789';

    // Start the final string
    var str = '';
    for(i = 1; i <= strLength; i++) {
        // Get a random charactert from the possibleCharacters string
        var randomCharacter = possibleCharacters.charAt(Math.floor(Math.random() * possibleCharacters.length));
        // Append this character to the string
        str+=randomCharacter;
    }
    // Return the final string
    return str;
  } else {
    return false;
  }
};

// Create new validators.json to update for new epochs
helpers.generateValidators = function() {
  //Dependencies
  const sqlite3 = require("sqlite3").verbose();
  const fs = require("fs");
  // open database from file
  let db = new sqlite3.Database("./scores.sqlite3", (err) => {
    if (err) {
      return console.error(err.message);
    }
    console.log("Connected to the SQlite database.");
  });

  // when sqlite is updated current query will be deprecated to "SELECT * FROM scores as s WHERE epoch > -1 ORDER BY epoch DESC, marinade_staked DESC"
  let sql =
    "SELECT * FROM scores as s WHERE epoch > -1 ORDER BY epoch DESC";
  let params = [];
  var holder = [];
  var vote_address_index = {}
  var index = -1;
  db.each(sql, params, (err, row) => {
    var vote_address = row.vote_address;
    if (vote_address_index[`${vote_address}`] == null) {
      index++;
      var toy = {};
      toy["validator_vote_address"] = row.vote_address;
      toy["keybase_id"] = row.keybase_id;
      toy["validator_description"] = row.name;
      toy["stats"] = [];
      holder.push(toy);
      var stats = {};
      stats["epoch"] = row.epoch;
      stats["score"] = row.score;
      stats["avg_position"] = row.avg_position;
      stats["commission"] = row.commission;
      stats["active_stake"] = row.active_stake;
      stats["epoch_credits"] = row.epoch_credits;
      stats["data_center_concentration"] = row.data_center_concentration;
      stats["can_halt_the_network_group"] = row.can_halt_the_network_group;
      stats["stake_state"] = row.stake_state;
      stats["stake_state_reason"] = row.stake_state_reason;
      stats["pct"] = row.pct;
      stats["stake_conc"] = row.stake_conc;
      stats["adj_credits"] = row.adj_credits;
      holder[index]["stats"].push(stats);
      vote_address_index[`${vote_address}`] = index
      
    } else {
      var stats = {};
      stats["epoch"] = row.epoch;
      stats["score"] = row.score;
      stats["avg_position"] = row.avg_position;
      stats["commission"] = row.commission;
      stats["active_stake"] = row.active_stake;
      stats["epoch_credits"] = row.epoch_credits;
      stats["data_center_concentration"] = row.data_center_concentration;
      stats["can_halt_the_network_group"] = row.can_halt_the_network_group;
      stats["stake_state"] = row.stake_state;
      stats["stake_state_reason"] = row.stake_state_reason;
      stats["pct"] = row.pct;
      stats["stake_conc"] = row.stake_conc;
      stats["adj_credits"] = row.adj_credits;
      holder[vote_address_index[`${vote_address}`]]["stats"].push(stats);
    }
  });

  // close the database connection
  db.close((err) => {
    if (err) {
      return console.error(err.message);
    }
    console.log("Closed the database connection.");
    jsonHolder = JSON.stringify(holder);
    fs.writeFileSync("validators.json", jsonHolder);
  });
}


// Export the module
module.exports = helpers;