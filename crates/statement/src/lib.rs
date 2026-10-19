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
//! No transparent addresses anywhere. Income arrives shielded (ZIP 213 lets
//! coinbase pay a shielded address; faucets and pools can pay one too) and
//! payouts are shielded. So there are two views of each epoch:
//!
//! * [`SplitStatement`]: the **auditor's** view. Every income and payout item,
//!   each provable on chain with the treasury's viewing keys. Never published.
//! * [`PublicStatement`]: the **public** view. Totals only, the policy, the
//!   hash chain, and a digest of the auditor's view. No txids, no amounts per
//!   item, no addresses. Refused when an epoch has too few payouts to hide in.
//!
//! What this does *not* prove: that every recipient is a distinct real person.
//! The treasury could pay itself and call it a rider. Ride attestations and
//! hardware-signed rides (IntelliGrip) narrow that; this module only proves
//! the money moved as the policy says.

use anyhow::{anyhow, bail, ensure, Result};
use minesplit_policy::Policy;
use serde::{Deserialize, Serialize};
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

/// One shielded income note (pool payout, coinbase or donation) received by the treasury.
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
    /// Public, non-secret treasury id: sha256 of its full viewing key, so
    /// statements from different treasuries can't be mixed.
    pub treasury: String,
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

/// Fewest payouts an epoch may publish totals for. With fewer, a total is
/// close to one person's income, so the epoch must be merged with the next.
pub const MIN_PUBLIC_PAYOUTS: usize = 5;

/// What the world sees. Totals, policy, chain links; nothing per payout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicStatement {
    pub v: String,
    pub label: String,
    pub network: String,
    pub epoch: u64,
    pub policy: Policy,
    pub treasury: String,
    pub income_range: HeightRange,
    pub payout_range: HeightRange,
    pub mined_zat: u64,
    pub carry_in_zat: u64,
    pub budget_zat: u64,
    pub paid_zat: u64,
    pub carry_out_zat: u64,
    /// sha256 of the auditor's full statement; the auditor signs this.
    pub audited_digest: String,
    /// The previous epoch's `audited_digest`, so the public chain mirrors the private one.
    pub prev_audited_digest: String,
}

pub const PUBLIC_VERSION: &str = "minesplit/public-statement/1";

impl SplitStatement {
    /// The publishable view. Errors rather than publish an epoch whose totals
    /// would be close to one person's payout.
    pub fn public(&self) -> Result<PublicStatement> {
        ensure!(
            self.paid.is_empty() || self.paid.len() >= MIN_PUBLIC_PAYOUTS,
            "only {} payouts this epoch; publishing the total would expose them. Merge with the next epoch.",
            self.paid.len()
        );
        Ok(PublicStatement {
            v: PUBLIC_VERSION.into(),
            label: self.label.clone(),
            network: self.network.clone(),
            epoch: self.epoch,
            policy: self.policy.clone(),
            treasury: self.treasury.clone(),
            income_range: self.income_range.clone(),
            payout_range: self.payout_range.clone(),
            mined_zat: self.mined_zat,
            carry_in_zat: self.carry_in_zat,
            budget_zat: self.budget_zat,
            paid_zat: self.paid_zat,
            carry_out_zat: self.carry_out_zat,
            audited_digest: self.digest(),
            prev_audited_digest: self.prev.clone(),
        })
    }
}

/// Checks a chain of public statements without any private data: the
/// arithmetic of each epoch and the links between them.
pub fn check_public(s: &PublicStatement, prev: Option<&PublicStatement>) -> Vec<String> {
    let mut bad = vec![];
    if s.v != PUBLIC_VERSION {
        bad.push("unknown public statement version".into());
    }
    if s.budget_zat != s.policy.rider_share(s.mined_zat) + s.carry_in_zat {
        bad.push("budget is not the published share of income plus carry".into());
    }
    if s.paid_zat + s.carry_out_zat != s.budget_zat {
        bad.push("paid + carried != budget".into());
    }
    match prev {
        None if s.epoch != 0 || s.carry_in_zat != 0 || !s.prev_audited_digest.is_empty() => {
            bad.push("first statement must be epoch 0 with no carry".into())
        }
        Some(p) => {
            if s.epoch != p.epoch + 1 {
                bad.push("epochs are not consecutive".into());
            }
            if s.carry_in_zat != p.carry_out_zat {
                bad.push("carry_in != previous carry_out: money went missing between epochs".into());
            }
            if s.prev_audited_digest != p.audited_digest {
                bad.push("public chain does not follow the audited chain".into());
            }
            if s.policy != p.policy || s.treasury != p.treasury {
                bad.push("policy or treasury changed mid-chain".into());
            }
        }
        _ => {}
    }
    bad
}

pub struct Inputs<'a> {
    pub label: &'a str,
    pub network: &'a str,
    pub epoch: u64,
    pub prev: Option<&'a SplitStatement>,
    pub policy: &'a Policy,
    pub treasury: &'a str,
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
        treasury: i.treasury.into(),
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
            if s.treasury != p.treasury {
                fail("treasury changed".into());
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
            treasury: "t1",
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
            treasury: "t1",
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
            treasury: "t1",
            income_range: HeightRange { from: 1, to: 10 },
            payout_range: HeightRange { from: 11, to: 20 },
            mined: vec![MinedItem { txid: "aa".repeat(32), height: 5, zat: 100 }],
            paid: vec![PaidItem { txid: "bb".repeat(32), height: 15, zat: 51 }],
        });
        assert!(r.is_err());
    }

    fn epoch_with(n: usize) -> SplitStatement {
        build(Inputs {
            label: "t",
            network: "test",
            epoch: 0,
            prev: None,
            policy: &policy(),
            treasury: "t1",
            income_range: HeightRange { from: 1, to: 100 },
            payout_range: HeightRange { from: 101, to: 200 },
            mined: vec![MinedItem { txid: "ab".repeat(32), height: 50, zat: 1_000_000_000 }],
            paid: (0..n)
                .map(|i| PaidItem { txid: format!("{:064x}", 0xfeed_0000 + i), height: 150, zat: 1_234_567 + i as u64 })
                .collect(),
        })
        .unwrap()
    }

    #[test]
    fn public_view_leaks_nothing_per_payout() {
        let s = epoch_with(6);
        let public = serde_json::to_string(&s.public().unwrap()).unwrap();
        for p in &s.paid {
            assert!(!public.contains(&p.txid), "payout txid leaked");
            assert!(!public.contains(&p.zat.to_string()), "payout amount leaked");
        }
        for m in &s.mined {
            assert!(!public.contains(&m.txid), "income txid leaked");
        }
        for key in ["\"paid\":[", "\"mined\":[", "address", "memo"] {
            assert!(!public.contains(key), "public statement contains {key}");
        }
    }

    #[test]
    fn small_epochs_are_not_published() {
        assert!(epoch_with(1).public().is_err());
        assert!(epoch_with(4).public().is_err());
        assert!(epoch_with(0).public().is_ok());
        assert!(epoch_with(5).public().is_ok());
    }

    #[test]
    fn public_chain_verifies_and_catches_skims() {
        let s0 = epoch_with(5);
        let p0 = s0.public().unwrap();
        assert!(check_public(&p0, None).is_empty());
        let mut p1 = p0.clone();
        p1.epoch = 1;
        p1.prev_audited_digest = p0.audited_digest.clone();
        p1.mined_zat = 0;
        p1.carry_in_zat = 0; // dropped the carry
        p1.budget_zat = 0;
        p1.paid_zat = 0;
        p1.carry_out_zat = 0;
        assert!(check_public(&p1, Some(&p0)).iter().any(|m| m.contains("went missing")));
    }
}
