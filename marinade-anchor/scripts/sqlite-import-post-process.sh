#!/bin/bash
echo Importing into table scores2
sqlite3 <<::END
.open ../stake-o-matic/db/score-sqlite3.db 

-- create table to receive data
DROP TABLE IF EXISTS post_process_imported;
CREATE TABLE post_process_imported(
  epoch INT,
  rank INT,
  score INTEGER, 
  name TEXT,
  credits_observed INTEGER,
  vote_address TEXT,
  commission INTEGER,
  average_position DOUBLE,
  data_center_concentration DOUBLE,
  avg_active_stake INTEGER, 
  apy DOUBLE,
  delinquent BOOL,
  this_epoch_credits INTEGER,
  pct DOUBLE,
  marinade_staked DOUBLE,
  should_have  DOUBLE,
  remove_level INTEGER,
  remove_level_reason TEXT,
  under_nakamoto_coefficient BOOLEAN,
  keybase_id TEXT,
  identity TEXT,
  stake_concentration DOUBLE,
  base_score INTEGER
);

-- import post_process data
.mode csv
.import post-process.csv post_process_imported
--remove header row
delete FROM post_process_imported where vote_address='vote_address';


-- create if not exists scores2
create table if not exists scores2 as select * from post_process_imported where 1=0;

-- store at scores2
delete from scores2
where epoch = (select distinct epoch from post_process_imported);

insert into scores2 
  (epoch ,  rank ,  score,   name ,  credits_observed ,  vote_address ,
  commission ,  average_position ,  data_center_concentration ,  avg_active_stake , 
  apy ,  delinquent ,  this_epoch_credits ,  pct ,  marinade_staked ,  should_have  ,
  remove_level ,  remove_level_reason,  under_nakamoto_coefficient ,  keybase_id ,  identity ,  stake_concentration ,  base_score )
select 
  epoch ,  rank ,  score,   name ,  credits_observed ,  vote_address ,
  commission ,  average_position ,  data_center_concentration ,  avg_active_stake , 
  apy ,  delinquent ,  this_epoch_credits ,  pct ,  marinade_staked ,  should_have  ,
  remove_level ,  remove_level_reason,  under_nakamoto_coefficient ,  keybase_id ,  identity ,  stake_concentration ,  base_score
  from post_process_imported
  ;

-- show top validators with pct assgined (informative)
.mode column
.headers ON
select count(*) from post_process_imported;

.quit
::END
