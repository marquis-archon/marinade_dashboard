#!/usr/bin/ruby

require 'json'

def start
    `killall solana-test-validator`
    `rm -r test-ledger`
    puts "Starting validator"
    spawn 'solana-test-validator --slots-per-epoch 500', :out=>"/dev/null"
    sleep 60

    puts `solana config set --url http://localhost:8899`
    puts `solana program deploy -v -u localhost --program-id ../../keys/marinade_finance-keypair.json ../../target/verifiable/marinade_finance.so`
    `rm -r keys`
    `mkdir keys`
    # `solana-keygen new -s --force --no-bip39-passphrase -o keys/instance.json`
    # `solana-keygen new -s --force --no-bip39-passphrase -o keys/st_sol_mint.json`
    # `solana-keygen new -s --force --no-bip39-passphrase -o keys/liq_pool_mint.json`
    `../../target/debug/mardmin-init init -c ../../keys/creator.json --output-instance keys/instance.pubkey`
    $?.success? or raise 'init error'

    File.open("keys/instance.pubkey").read
end

def generate_stakes(count, without_lockup_count)
    validators = JSON.parse(`solana validators --output json`)
    validator = validators["validators"][0]["voteAccountPubkey"]
    epoch = current_epoch
    for i in 0...count
      `solana-keygen new -s --force --no-bip39-passphrase -o keys/stake#{i}.json`
      puts "Generate stake: " + `solana-keygen pubkey keys/stake#{i}.json`
      `solana create-stake-account keys/stake#{i}.json 2`
      `solana delegate-stake keys/stake#{i}.json #{validator}`
      if i >= without_lockup_count
        puts "With lockup"
        `solana stake-set-lockup keys/stake#{i}.json --new-custodian keys/stake#{i}.json --lockup-epoch #{epoch + 4}`
      end
    end
end
 

def current_epoch
    `solana epoch`.to_i
end

def wait_for_epoch target
    while current_epoch < target
        sleep 10
    end
end

def deposit_stake_accounts(instance, i, must_fail = false)
  puts "Deposit stake account ##{i} #{`solana-keygen pubkey keys/stake#{i}.json`}"
  puts `../../target/debug/marinade -i #{instance} deposit-stake-account #{`solana-keygen pubkey keys/stake#{i}.json`}`
  if must_fail
    (not $?.success?) or raise 'deposit_stake_accounts must fail'
  else
    $?.success? or raise 'deposit_stake_accounts error'
  end
end

instance = start
puts `../../target/debug/validator-manager -i #{instance} add-validator`
$?.success? or raise 'validator-manager error'
generate_stakes(10, 5);
wait_for_epoch(current_epoch + 3)
for i in 0...5
  deposit_stake_accounts(instance, i)
end
for i in 5...10
  deposit_stake_accounts(instance, i, true)
end
wait_for_epoch(current_epoch + 2)
for i in 5...10
  deposit_stake_accounts(instance, i)
end

sleep 30
puts `../../target/debug/marcrank -i #{instance} update-price`
$?.success? or raise 'update-price error'
puts `../../target/debug/marcrank -i #{instance} merge-stakes`
$?.success? or raise 'merge-stakes error'
puts `../../target/debug/marinade -i #{instance} -v show`

puts `../../target/debug/marinade -i #{instance} order-unstake 5`
$?.success? or raise 'order-unstake error'
sleep 30
puts `../../target/debug/marcrank -i #{instance} update-price`
$?.success? or raise 'update-price error'
puts `../../target/debug/marcrank -i #{instance} -v stake-delta`
$?.success? or raise 'stake-delta error'
puts `../../target/debug/marinade -i #{instance} -v show`
wait_for_epoch(current_epoch + 1)
puts `../../target/debug/marcrank -i #{instance} update-price`
$?.success? or raise 'update-price error'
puts `../../target/debug/marinade -i #{instance} -v show`