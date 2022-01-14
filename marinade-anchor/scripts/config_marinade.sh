#!/bin/bash
set -e
../target/debug/mardmin -i $1 config-marinade ${@:2} -p /tmp/tx
multisig propose-binary-transaction --multisig-address 7mSA2bgzmUCi4wh16NQEfT76XMqJULni6sheZRCjcyx7 --data /tmp/tx