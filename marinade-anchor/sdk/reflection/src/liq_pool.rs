use crate::FeeDef;
use marinade_finance_offchain_sdk::marinade_finance::Fee;
use marinade_finance_offchain_sdk::solana_sdk::pubkey::Pubkey;
use serde::Serialize;

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct LiqPool {
    pub lp_mint: Pubkey,
    pub actual_sol_amount: u64,
    pub actual_msol_amount: u64,

    //The next 3 values define the SOL/mSOL Liquidity pool fee curve params
    // We assume this pool is always UNBALANCED, there should be more SOL than mSOL 99% of the time
    ///Liquidity target. If the Liquidity reach this amount, the fee reaches lp_min_discount_fee
    pub lp_liquidity_target: u64, // 10_000 SOL initially
    /// Liquidity pool max fee
    #[serde(with = "FeeDef")]
    pub lp_max_fee: Fee, //3% initially
    /// SOL/mSOL Liquidity pool min fee
    #[serde(with = "FeeDef")]
    pub lp_min_fee: Fee, //0.3% initially
    /// Treasury cut
    #[serde(with = "FeeDef")]
    pub treasury_cut: Fee, //2500 => 25% how much of the Liquid unstake fee goes to treasury_msol_account

    pub lp_supply: u64, // virtual lp token supply. May be > real supply because of burning tokens. Use UpdateLiqPool to align it with real value
    pub actual_lp_supply: u64,
    pub lent_from_sol_leg: u64,
}
