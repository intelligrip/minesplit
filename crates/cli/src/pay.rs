//! Turns an epoch's verified claims into ONE batched shielded payment request
//! (ZIP 321) that zcash-devtool's `wallet pay` executes.

use std::collections::HashSet;

use anyhow::{bail, ensure, Result};
use base64::Engine;
use minesplit_policy::{plan_epoch, EpochPlan, Policy, Ride};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::orchard_only;

/// One verified claim: who (pseudonym), how far, and a fresh address the
/// recipient generated for this payout only.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub rider: String,
    pub meters: u64,
    pub address: String,
}

/// Memo: a random receipt id and nothing else. No names, places or times.
pub fn receipt_memo() -> String {
    let mut b = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut b);
    format!("minesplit r={}", hex::encode(b))
}

pub fn memo_is_clean(m: &str) -> bool {
    let rest = match m.strip_prefix("minesplit r=") {
        Some(r) => r,
        None => return false,
    };
    rest.len() == 16 && rest.chars().all(|c| c.is_ascii_hexdigit())
}

fn zec(z: u64) -> String {
    let s = format!("{}.{:08}", z / 100_000_000, z % 100_000_000);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub struct Batch {
    pub plan: EpochPlan,
    pub uri: String,
    pub payments: usize,
}

/// `used` holds every address the treasury has already paid; reuse is refused.
pub fn build(policy: &Policy, budget_zat: u64, claims: &[Claim], used: &HashSet<String>) -> Result<Batch> {
    let mut seen = HashSet::new();
    let mut addrs = Vec::with_capacity(claims.len());
    for c in claims {
        let a = orchard_only(&c.address)?;
        if used.contains(&a) || used.contains(&c.address) || !seen.insert(a.clone()) {
            bail!("claim {}: address was used before; every payout needs a fresh address", c.id);
        }
        addrs.push(a);
    }
    let rides: Vec<Ride> = claims
        .iter()
        .map(|c| Ride { id: c.id.clone(), rider: c.rider.clone(), meters: c.meters })
        .collect();
    let plan = plan_epoch(policy, budget_zat, &rides);

    let mut parts = vec![];
    let mut n = 0;
    for (p, a) in plan.payouts.iter().zip(&addrs) {
        if p.paid_zat == 0 {
            continue;
        }
        let memo = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(receipt_memo());
        let sfx = if n == 0 { String::new() } else { format!(".{n}") };
        parts.push(format!("address{sfx}={a}&amount{sfx}={}&memo{sfx}={memo}", zec(p.paid_zat)));
        n += 1;
    }
    ensure!(n > 0, "nothing to pay this epoch");
    Ok(Batch { plan, uri: format!("zcash:?{}", parts.join("&")), payments: n })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memos_carry_only_a_receipt_id() {
        for _ in 0..50 {
            assert!(memo_is_clean(&receipt_memo()));
        }
        assert!(!memo_is_clean("minesplit r=abc"));
        assert!(!memo_is_clean("thanks Sam for the ride to work"));
    }

    #[test]
    fn zec_formatting() {
        assert_eq!(zec(100_000_000), "1");
        assert_eq!(zec(12_345), "0.00012345");
        assert_eq!(zec(10_000), "0.0001");
    }
}
