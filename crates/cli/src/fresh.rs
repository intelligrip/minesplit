//! Fresh Orchard-only payout addresses, derived from a recipient's viewing key.
//!
//! Each address uses a random 88-bit diversifier, so addresses are unlinkable
//! to each other and to the wallet's own sequence, yet the recipient's wallet
//! finds every payment with its incoming viewing key. No transparent or
//! Sapling receivers are ever created, so nothing is reserved on the
//! transparent side either.

use anyhow::{anyhow, Context, Result};
use orchard::keys::Scope;
use rand::RngCore;
use zcash_address::unified::{self, Encoding};
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_protocol::consensus::{MainNetwork, NetworkType, TestNetwork};
use zip32::DiversifierIndex;

pub fn fresh_addresses(ufvk: &str, network: &str, count: usize) -> Result<Vec<String>> {
    let (net, fvk) = match network {
        "main" => (NetworkType::Main, UnifiedFullViewingKey::decode(&MainNetwork, ufvk.trim())),
        "test" => (NetworkType::Test, UnifiedFullViewingKey::decode(&TestNetwork, ufvk.trim())),
        n => return Err(anyhow!("network must be main or test, not {n}")),
    };
    let fvk = fvk.map_err(|e| anyhow!("bad UFVK: {e}"))?;
    let ofvk = fvk.orchard().context("this viewing key has no Orchard component")?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut d = [0u8; 11];
        rand::thread_rng().fill_bytes(&mut d);
        let addr = ofvk.address_at(DiversifierIndex::from(d), Scope::External);
        let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(addr.to_raw_address_bytes())])?;
        out.push(ua.encode(&net));
    }
    Ok(out)
}
