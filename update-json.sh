# Update scores table
cd stake-o-matic
cargo build 
export VALIDATORS_APP_TOKEN=5TFNgpCnuRZz6mkk3oyNFbin
bash clean-score-all-mainnet.sh
cd ..


# Update scores2 table using scores table
cd marinade-anchor 
bash update-scores.sh