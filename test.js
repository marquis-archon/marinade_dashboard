const sqlite3 = require('sqlite3').verbose();
const fs = require('fs');

// open database from file
let db = new sqlite3.Database('./scores.sqlite3', (err) => {
  if (err) {
    return console.error(err.message);
  }
  console.log('Connected to the SQlite database.');
});

//query
let sql = 'SELECT * FROM scores as s WHERE epoch > -1 ORDER BY identity, epoch';
let params = [];
var holder = [];
var vote_count = new Set();
var index = -1;
db.each(sql,params, (err, row) => {
    var vote_address = row.vote_address;
    if(!vote_count.has(vote_address)){
      index++;
      vote_count.add(vote_address);
      var toy = {};
      toy["validator_vote_address"] = row.vote_address;
      toy["keybase_id"] = row.keybase_id;
      toy["validator_description"] = row.name;
      toy["stats"] = [];
      toy["latest_data"] = vote_count;
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
      stats['stake_conc'] = row.stake_conc;
      stats["adj_credits"] = row.adj_credits;
      holder[index]["stats"].push(stats);
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
      stats['stake_conc'] = row.stake_conc;
      stats["adj_credits"] = row.adj_credits;
      holder[index]["stats"].push(stats);
    }
 }); 
/*
//query2
let sql2 = 'SELECT vote_address FROM temp WHERE epoch > -1 ORDER BY vote_address';
//let params = [];
var new_holder = {};
var vote_count = new Set();
var index = -1;
db.each(sql2, params, (err, row) => {
    console.log(row);
    if(!(row.vote_address in new_holder)){
        console.log(row);
        new_holder[row.vote_address] = [];
        new_holder[row.vote_address].push();
    } else {
        new_holder[row.vote_address].push();
    }
}); 

// close the database connection
db.close((err) => {
  if (err) {
    return console.error(err.message);
  }
  console.log('Closed the database connection.');
  jsonHolder = JSON.stringify(holder);
  fs.writeFileSync('validators2.json',jsonHolder);
});*/