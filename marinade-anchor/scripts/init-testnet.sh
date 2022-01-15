#!/bin/bash
set -xe
solana config set -ut
#solana transfer AufL1ZuuAZoX7jBw8kECvjUYjfhWqZm13hbXeqnLMhFu 3 --allow-unfunded-recipient
./mardmin-init init \
    -i keys/instance.json \
    -m ~/.config/solana/mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So.json \
    -l ~/.config/solana/LPmSozJJ8Jh69ut2WP3XmVohTjL4ipR18yiCzxrUmVj.json \
    -t ~/.config/solana/AufL1ZuuAZoX7jBw8kECvjUYjfhWqZm13hbXeqnLMhFu.json