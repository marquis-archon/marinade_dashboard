#![cfg_attr(not(debug_assertions), deny(warnings))]

pub mod instruction_helpers;

use std::ops::{Deref, DerefMut};

use marinade_finance_onchain_sdk::marinade_finance::located::Located;
use solana_sdk::pubkey::Pubkey;

pub use anchor_lang;
pub use anchor_spl;
pub use marinade_finance_onchain_sdk::*;
pub use solana_offchain_common::*;
pub use spl_associated_token_account;
pub use spl_token;
/*
 * Pair of some type T and Pubkey used as Located<T>
 * Usable in CLI when we create object and wanna serialize it into newly created account later
 */
pub struct WithKey<T> {
    inner: T,
    pub key: Pubkey,
}

impl<T> WithKey<T> {
    pub fn new(inner: T, key: Pubkey) -> Self {
        Self { inner, key }
    }

    pub fn replace(&mut self, inner: T) -> T {
        std::mem::replace(&mut self.inner, inner)
    }
}

impl<T> Located<T> for WithKey<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }

    fn as_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    fn key(&self) -> Pubkey {
        self.key
    }
}

impl<T> Deref for WithKey<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for WithKey<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
