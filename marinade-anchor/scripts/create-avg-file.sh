#!/bin/bash
sqlite3 <<::END
.open ../stake-o-matic/db/score-sqlite3.db 
.headers on
.mode csv
.output avg.csv
SELECT rank, pct,
    epoch,
    keybase_id,
    name,
    vote_address,
    case when pct>0 then cast(avg_score as INTEGER) else 0 end as score,
    avg_pos as average_position,
    cast(avg_ec as INTEGER) as epoch_credits,
	cast(avg_commiss as INTEGER) as commission,
    dcc2 as data_center_concentration,
    base_score, mult, avg_score, avg_active_stake,
    identity, can_halt_the_network_group, stake_conc
 FROM AVG
 order by avg_score desc;
.quit
::END
