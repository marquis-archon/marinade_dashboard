use std::collections::BTreeMap;

use marinade_finance_offchain_sdk::anchor_lang::prelude::Pubkey;
use serde::{ser::SerializeMap, Serialize, Serializer};

pub fn serialize<S, T>(map: &BTreeMap<Pubkey, T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    let mut serializer = serializer.serialize_map(Some(map.len()))?;
    for (key, value) in map {
        serializer.serialize_entry(&key.to_string(), value)?;
    }
    serializer.end()
}
