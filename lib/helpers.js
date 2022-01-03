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
        // Get a random characters from the possibleCharacters string
        var randomCharacter = possibleCharacters.charAt(crypto.randomInt(possibleCharacters.length));
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

  /**
   * Grab all the historical data based on the ~400 validators in the most recent epoch from scores
   * UNION ALL (combine query)
   * Grab all the validators for each epoch with marinade_staked > 0 from scores2
   */
  let sql =
    "SELECT scores.epoch as epoch, scores.name, scores2.vote_address,  scores2.rank as rank, scores.pct as pct, scores2.score, scores2.commission, scores2.avg_active_stake as average_active_stake, scores2.apy, scores2.delinquent, scores2.marinade_staked, scores2.should_have, scores2.data_center_concentration \
    FROM scores2 \
    INNER JOIN scores ON scores.vote_address = scores2.vote_address \
    WHERE scores2.marinade_staked > 0  AND scores2.epoch = (SELECT MAX(scores2.epoch) FROM scores2) AND scores.epoch > 260 \
    UNION ALL \
    SELECT scores.epoch as epoch, scores.name, scores2.vote_address,  NULL as rank, scores.pct as pct, scores.score, scores.commission, scores.avg_position as average_active_stake, NULL as apy, NULL as delinquent, NULL as marinade_staked, NULL as should_have, scores.data_center_concentration \
    FROM scores2 \
    INNER JOIN scores ON scores.vote_address = scores2.vote_address \
    WHERE scores2.marinade_staked > 0  AND scores2.epoch = (SELECT MAX(scores2.epoch) FROM scores2) AND scores.epoch <= 260 \
    ORDER BY epoch ASC, rank ASC ";
  let params = [];
  var holder = [];
  var vote_address_index = {}
  var index = -1;
  db.each(sql, params, (err, row) => {
    var vote_address = row.vote_address;
    if (vote_address_index[`${vote_address}`] == null) {
      index++;
      var validator_stats = {};
      validator_stats["validator_vote_address"] = row.vote_address;
      validator_stats["keybase_id"] = row.keybase_id;
      validator_stats["validator_description"] = row.name;
      validator_stats["epoch_stats"] = [];
      holder.push(validator_stats);
      var epoch_stats = {};
      epoch_stats["epoch"] = row.epoch;
      epoch_stats["score"] = row.score;
      epoch_stats["credits_observed"] = row.credits_observed;
      epoch_stats["rank"] = row.rank;
      epoch_stats["vote_address"] = row.vote_address;
      epoch_stats["commission"] = row.commission;
      epoch_stats["avg_position"] = row.average_position;
      epoch_stats["data_center_concentration"] = row.data_center_concentration;
      epoch_stats["avg_active_stake"] = row.avg_active_stake;
      epoch_stats["apy"] = row.apy;
      epoch_stats["delinquent"] = row.delinquent;
      epoch_stats["this_epoch_credits"] = row.this_epoch_credits;
      epoch_stats["pct"] = row.pct;
      epoch_stats["marinade_staked"] = row.marinade_staked;
      epoch_stats["should_have"] = row.should_have;
      epoch_stats["remove_level"] = row.remove_level;
      epoch_stats["remove_level_reason"] = row.remove_level_reason;

      // Deprecated for scores2
      // stats["can_halt_the_network_group"] = row.can_halt_the_network_group;
      // stats["stake_state"] = row.stake_state;
      // stats["stake_state_reason"] = row.stake_state_reason;
      // stats["pct"] = row.pct;
      // stats["stake_conc"] = row.stake_conc;
      // stats["adj_credits"] = row.adj_credits;
      holder[index]["epoch_stats"].push(epoch_stats);
      vote_address_index[`${vote_address}`] = index
      
    } else {
      var epoch_stats = {};
      epoch_stats["epoch"] = row.epoch;
      epoch_stats["rank"] = row.rank;
      epoch_stats["score"] = row.score;
      epoch_stats["credits_observed"] = row.credits_observed;
      epoch_stats["vote_address"] = row.vote_address;
      epoch_stats["commission"] = row.commission;
      epoch_stats["avg_position"] = row.average_position;
      epoch_stats["data_center_concentration"] = row.data_center_concentration;
      epoch_stats["avg_active_stake"] = row.avg_active_stake;
      epoch_stats["apy"] = row.apy;
      epoch_stats["delinquent"] = row.delinquent;
      epoch_stats["this_epoch_credits"] = row.this_epoch_credits;
      epoch_stats["pct"] = row.pct;
      epoch_stats["marinade_staked"] = row.marinade_staked;
      epoch_stats["should_have"] = row.should_have;
      epoch_stats["remove_level"] = row.remove_level;
      epoch_stats["remove_level_reason"] = row.remove_level_reason;

      // Deprecated for scores2
      // stats["can_halt_the_network_group"] = row.can_halt_the_network_group;
      // stats["stake_state"] = row.stake_state;
      // stats["stake_state_reason"] = row.stake_state_reason;
      // stats["pct"] = row.pct;
      // stats["stake_conc"] = row.stake_conc;
      // stats["adj_credits"] = row.adj_credits;
      holder[vote_address_index[`${vote_address}`]]["epoch_stats"].push(epoch_stats);
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