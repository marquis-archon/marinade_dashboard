export RUST_LOG=solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=debug,solana_bpf_loader=debug,solana_rbpf=debug
#export RUST_LOG=solana_metrics=info,debug
cargo test-bpf