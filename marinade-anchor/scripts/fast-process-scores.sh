#!/bin/bash
# ONLY PROCESS SCORES into scores2 -- BUT NO UPDATE ON-CHAIN --- use update-scores
# set -ex
# export EPOCH=$(solana epoch-info|sed -n 's/Epoch: //p')
# bash scripts/create-avg-file.sh
# ##curl https://stakeview.app/apy/$(($EPOCH-1)).json >temp/apy_data.json
# solana validators --output json >temp/solana-validators.json
# curl https://stakeview.app/apy/$EPOCH.json >temp/apy_data.json
target/debug/score-post-process $1 process-scores --apy-file temp/apy_data.json $2 $3 $4
#>../staking-status/$EPOCH-update-scores.log
# add post-process.csv to scores2 table in the sqlite-database
bash scripts/sqlite-import-post-process.sh
