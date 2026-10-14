//! Split statements.
//!
//! The promise a miner-funded treasury makes: *every zatoshi of the riders'
//! share is either paid to riders or carried forward for riders.* A split
//! statement makes that checkable for one epoch:
//!
//! ```text
//!   budget  = rider_share(mined last epoch) + carry_in
//!   budget == paid_to_riders + carry_out          (nothing leaks out)
//!   carry_in == previous statement's carry_out    (nothing disappears between epochs)
//! ```
//!
//! * **Mined income** is public by construction: pool payouts land on the
//!   treasury's published transparent mining address, and anyone can sum them.
//! * **Rider payouts** are shielded. They are proven with a sender-side audit
//!   statement (`ghost audit verify`): every payout is a real on-chain note,
//!   recovered with the treasury's outgoing viewing key. No rider keys, routes
//!   or identities are involved.
//!
//! What this does *not* prove: that every recipient is a distinct real person.
//! The treasury could pay itself and call it a rider. Ride attestations and
//! hardware-signed rides (IntelliGrip) narrow that; this module only proves
//! the money moved as the policy says.

use anyhow::{anyhow, bail, ensure, Result};
use minesplit_policy::Policy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const STATEMENT_VERSION: &str = "minesplit/split-statement/1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeightRange {
    pub from: u32,
    pub to: u32,
}

impl HeightRange {
    pub fn contains(&self, h: u32) -> bool {
        self.from <= h && h <= self.to
    }
}

/// One pool payout (or solo coinbase) received on the published mining address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinedItem {
    pub txid: String,
    pub height: u32,
    pub zat: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaidItem {
    pub txid: String,
    pub height: u32,
    pub zat: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitStatement {
    pub v: String,
    pub label: String,
    pub network: String,
    pub epoch: u64,
    /// sha256 of the previous statement's canonical JSON; empty for epoch 0.
    pub prev: String,
    pub policy: Policy,
    pub mining_address: String,
    pub income_range: HeightRange,
    pub payout_range: HeightRange,
    pub mined: Vec<MinedItem>,
    pub paid: Vec<PaidItem>,
    pub mined_zat: u64,
    pub carry_in_zat: u64,
    pub budget_zat: u64,
    pub paid_zat: u64,
    pub carry_out_zat: u64,
}

impl SplitStatement {
    pub fn digest(&self) -> String {
        // serde_json writes struct fields in declaration order, so this is canonical.
        hex::encode(Sha256::digest(serde_json::to_vec(self).expect("serializable")))
    }
}

/// Reads the payout items out of a `ghost audit verify --out` statement.
/// Refuses unless every payout in it verified.
pub fn paid_from_audit(audit: &Value) -> Result<Vec<PaidItem>> {
    ensure!(
        audit["v"] == "ghost-commute/audit-statement/1",
        "not a ghost audit statement"
    );
    ensure!(audit["all_verified"] == true, "audit statement has unverified payouts");
    audit["items"]
        .as_array()
        .ok_or_else(|| anyhow!("audit statement has no items (was it redacted?)"))?
        .iter()
        .map(|it| {
            Ok(PaidItem {
                txid: it["txid"].as_str().ok_or_else(|| anyhow!("item without txid"))?.into(),
                height: it["mined_height"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("item without mined height"))? as u32,
                zat: it["value_zat"].as_u64().ok_or_else(|| anyhow!("item without value"))?,
            })
        })
        .collect()
}

pub struct Inputs<'a> {
    pub label: &'a str,
    pub network: &'a str,
    pub epoch: u64,
    pub prev: Option<&'a SplitStatement>,
    pub policy: &'a Policy,
    pub mining_address: &'a str,
    pub income_range: HeightRange,
    pub payout_range: HeightRange,
    pub mined: Vec<MinedItem>,
    pub paid: Vec<PaidItem>,
}

/// Builds the statement for one epoch. Errors if the treasury broke its own rule.
pub fn build(i: Inputs) -> Result<SplitStatement> {
    i.policy.validate().map_err(|e| anyhow!(e))?;
    let carry_in = i.prev.map(|p| p.carry_out_zat).unwrap_or(0);
    let mined_zat: u64 = i.mined.iter().map(|m| m.zat).sum();
    let paid_zat: u64 = i.paid.iter().map(|p| p.zat).sum();
    let budget = i.policy.rider_share(mined_zat) + carry_in;
    if paid_zat > budget {
        // Paying riders more than promised is allowed by the spirit of the rule,
        // but then the extra came from somewhere else; record it as zero carry.
        bail!("paid {paid_zat} zat exceeds the rider budget {budget} zat; top-ups belong in their own statement");
    }
    let s = SplitStatement {
        v: STATEMENT_VERSION.into(),
        label: i.label.into(),
        network: i.network.into(),
        epoch: i.epoch,
        prev: i.prev.map(|p| p.digest()).unwrap_or_default(),
        policy: i.policy.clone(),
        mining_address: i.mining_address.into(),
        income_range: i.income_range,
        payout_range: i.payout_range,
        mined: i.mined,
        paid: i.paid,
        mined_zat,
        carry_in_zat: carry_in,
        budget_zat: budget,
        paid_zat,
        carry_out_zat: budget - paid_zat,
    };
    let problems = check(&s, i.prev);
    ensure!(problems.is_empty(), "statement fails its own checks: {problems:?}");
    Ok(s)
}

/// Every rule a verifier checks, given the statement and its predecessor.
/// Chain data (that each mined/paid txid exists at that height, pays that
/// amount) is checked separately by the `chain` layer; this is the arithmetic
/// and the links between epochs.
pub fn check(s: &SplitStatement, prev: Option<&SplitStatement>) -> Vec<String> {
    let mut bad = vec![];
    let mut fail = |m: String| bad.push(m);
    if s.v != STATEMENT_VERSION {
        fail("unknown statement version".into());
    }
    if let Err(e) = s.policy.validate() {
        fail(format!("policy: {e}"));
    }
    match prev {
        None => {
            if s.epoch != 0 || !s.prev.is_empty() || s.carry_in_zat != 0 {
                fail("first statement must be epoch 0 with no carry".into());
            }
        }
        Some(p) => {
            if s.prev != p.digest() {
                fail("prev hash does not match the previous statement".into());
            }
            if s.epoch != p.epoch + 1 {
                fail("epochs are not consecutive".into());
            }
            if s.carry_in_zat != p.carry_out_zat {
                fail(format!(
                    "carry_in {} != previous carry_out {}: rider money went missing between epochs",
                    s.carry_in_zat, p.carry_out_zat
                ));
            }
            if s.policy != p.policy {
                fail("policy changed mid-chain; a new policy needs a new chain".into());
            }
            if s.mining_address != p.mining_address {
                fail("mining address changed".into());
            }
            if s.income_range.from != p.income_range.to + 1 {
                fail("income ranges leave a gap or overlap".into());
            }
            if s.payout_range.from != p.payout_range.to + 1 {
                fail("payout ranges leave a gap or overlap".into());
            }
        }
    }
    let mined: u64 = s.mined.iter().map(|m| m.zat).sum();
    let paid: u64 = s.paid.iter().map(|p| p.zat).sum();
    if mined != s.mined_zat {
        fail("mined total does not match its items".into());
    }
    if paid != s.paid_zat {
        fail("paid total does not match its items".into());
    }
    if s.budget_zat != s.policy.rider_share(s.mined_zat) + s.carry_in_zat {
        fail("budget is not the published share of mined income plus carry".into());
    }
    if s.paid_zat + s.carry_out_zat != s.budget_zat {
        fail(format!(
            "paid {} + carried {} != budget {}: part of the riders' share is unaccounted for",
            s.paid_zat, s.carry_out_zat, s.budget_zat
        ));
    }
    for m in &s.mined {
        if !s.income_range.contains(m.height) {
            fail(format!("mined tx {} is outside the income range", short(&m.txid)));
        }
    }
    for p in &s.paid {
        if !s.payout_range.contains(p.height) {
            fail(format!("payout tx {} is outside the payout range", short(&p.txid)));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for t in s.mined.iter().map(|m| &m.txid).chain(s.paid.iter().map(|p| &p.txid)) {
        if !seen.insert(t) {
            fail(format!("tx {} listed twice", short(t)));
        }
    }
    bad
}

fn short(t: &str) -> &str {
    &t[..t.len().min(16)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy() -> Policy {
        Policy {
            version: 1,
            rider_share_bps: 5_000,
            base_rate_zat_per_km: 10_000,
            max_zat_per_ride: 500_000,
            max_zat_per_rider_epoch: 1_000_000,
        }
    }

    fn epoch0() -> SplitStatement {
        build(Inputs {
            label: "test",
            network: "regtest",
            epoch: 0,
            prev: None,
            policy: &policy(),
            mining_address: "tmMiner",
            income_range: HeightRange { from: 1, to: 100 },
            payout_range: HeightRange { from: 101, to: 200 },
            mined: vec![
                MinedItem { txid: "aa".repeat(32), height: 10, zat: 312_500_000 },
                MinedItem { txid: "bb".repeat(32), height: 20, zat: 312_500_000 },
            ],
            paid: vec![PaidItem { txid: "cc".repeat(32), height: 150, zat: 100_000_000 }],
        })
        .unwrap()
    }

    #[test]
    fn honest_epoch_balances() {
        let s = epoch0();
        assert_eq!(s.budget_zat, 312_500_000);
        assert_eq!(s.carry_out_zat, 212_500_000);
        assert!(check(&s, None).is_empty());
    }

    #[test]
    fn chain_carries_forward() {
        let s0 = epoch0();
        let s1 = build(Inputs {
            label: "test",
            network: "regtest",
            epoch: 1,
            prev: Some(&s0),
            policy: &policy(),
            mining_address: "tmMiner",
            income_range: HeightRange { from: 101, to: 200 },
            payout_range: HeightRange { from: 201, to: 300 },
            mined: vec![],
            paid: vec![PaidItem { txid: "dd".repeat(32), height: 250, zat: 212_500_000 }],
        })
        .unwrap();
        assert_eq!(s1.carry_out_zat, 0);
        assert!(check(&s1, Some(&s0)).is_empty());
    }

    #[test]
    fn skimming_the_carry_is_caught() {
        let s0 = epoch0();
        let mut s1 = s0.clone();
        s1.epoch = 1;
        s1.prev = s0.digest();
        s1.income_range = HeightRange { from: 101, to: 200 };
        s1.payout_range = HeightRange { from: 201, to: 300 };
        s1.mined = vec![];
        s1.paid = vec![];
        s1.mined_zat = 0;
        s1.paid_zat = 0;
        s1.carry_in_zat = 0; // quietly dropped 2.125 ZEC of rider money
        s1.budget_zat = 0;
        s1.carry_out_zat = 0;
        let bad = check(&s1, Some(&s0));
        assert!(bad.iter().any(|m| m.contains("went missing")), "{bad:?}");
    }

    #[test]
    fn inflated_carry_is_caught() {
        let mut s = epoch0();
        s.carry_out_zat += 1;
        assert!(check(&s, None).iter().any(|m| m.contains("unaccounted")));
    }

    #[test]
    fn edited_payout_amount_is_caught() {
        let mut s = epoch0();
        s.paid[0].zat += 1;
        assert!(!check(&s, None).is_empty());
    }

    #[test]
    fn overpaying_is_refused_at_build() {
        let r = build(Inputs {
            label: "t",
            network: "regtest",
            epoch: 0,
            prev: None,
            policy: &policy(),
            mining_address: "tm",
            income_range: HeightRange { from: 1, to: 10 },
            payout_range: HeightRange { from: 11, to: 20 },
            mined: vec![MinedItem { txid: "aa".repeat(32), height: 5, zat: 100 }],
            paid: vec![PaidItem { txid: "bb".repeat(32), height: 15, zat: 51 }],
        });
        assert!(r.is_err());
    }

    #[test]
    fn reads_ghost_audit_statement() {
        let audit = json!({
            "v": "ghost-commute/audit-statement/1",
            "all_verified": true,
            "items": [{"txid": "ee".repeat(32), "mined_height": 3374485, "value_zat": 9840, "verified": true}]
        });
        let p = paid_from_audit(&audit).unwrap();
        assert_eq!(p[0].zat, 9840);
        let mut bad = audit.clone();
        bad["all_verified"] = json!(false);
        assert!(paid_from_audit(&bad).is_err());
    }
}
