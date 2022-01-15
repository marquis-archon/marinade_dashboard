#![cfg_attr(not(debug_assertions), deny(warnings))]

use marinade_finance_offchain_sdk::anchor_lang::prelude::Pubkey;
use rand::{Rng, RngCore};

use marinade_finance_offchain_sdk::marinade_finance::Fee;
use serde::{Deserialize, Serialize};

pub mod accounts_builder;
pub mod builder;
pub mod liq_pool;
pub mod marinade;
pub mod pubkey_map;

pub fn random_pubkey(rng: &mut impl RngCore) -> Pubkey {
    loop {
        let result = Pubkey::new_from_array(rng.gen());
        if !result.is_on_curve() || !result.is_native_program_id() {
            return result;
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Fee")]
pub struct FeeDef {
    pub basis_points: u32,
}
