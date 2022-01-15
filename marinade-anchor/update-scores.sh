#!/bin/bash
set -ex
solana address
export EPOCH=$(solana epoch-info|sed -n 's/Epoch: //p')

# create avg.csv file from sqlite database
bash scripts/create-avg-file.sh

# get solana validators info as json file for current epoch_credits
solana validators --output json >temp/solana-validators.json

# get stakeview.app info for current APY
##curl https://stakeview.app/apy/$(($EPOCH-1)).json >temp/apy_data.json
curl https://stakeview.app/apy/$EPOCH.json >temp/apy_data.json

# post-process avg.csv generating post-process.csv
target/debug/score-post-process process-scores avg.csv --apy-file temp/apy_data.json
#>../staking-status/$EPOCH-update-scores.log

# use post-process.csv to update scores on-chain
# target/debug/validator-manager $1 update-scores --scores-file post-process.csv $2 $3 $4 >temp/update-output
#>../staking-status/$EPOCH-update-scores.log

# add post-process.csv to scores2 table in the sqlite-database
bash scripts/sqlite-import-post-process.sh