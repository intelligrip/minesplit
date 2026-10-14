//! The published payout rule.
//!
//! Miners fund a shielded treasury. A fixed, public share of what was mined in
//! one epoch becomes the rider budget for the next epoch. Each verified ride
//! asks for `distance × base rate` (capped). If the asks fit in the budget,
//! everyone is paid in full and the rest carries over. If they don't, every
//! ride is scaled down by the same factor, so the treasury never promises more
//! than the miners earned.
//!
//! Everything is integer zatoshi math, so anyone re-running the plan gets the
//! same numbers to the last zat.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 10_000 basis points = 100%.
pub const BPS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub version: u32,
    /// Share of mined ZEC owed to riders, in basis points (5000 = 50%).
    pub rider_share_bps: u64,
    /// Ask per kilometre before any scaling.
    pub base_rate_zat_per_km: u64,
    /// Cap on a single ride's ask.
    pub max_zat_per_ride: u64,
    /// Cap on one rider's total asks in one epoch.
    pub max_zat_per_rider_epoch: u64,
}

impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        if self.rider_share_bps > BPS {
            return Err("rider_share_bps above 100%".into());
        }
        if self.max_zat_per_ride == 0 || self.max_zat_per_rider_epoch == 0 {
            return Err("caps must be non-zero".into());
        }
        Ok(())
    }

    /// The riders' share of `mined_zat`, rounded down.
    pub fn rider_share(&self, mined_zat: u64) -> u64 {
        ((mined_zat as u128 * self.rider_share_bps as u128) / BPS as u128) as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ride {
    pub id: String,
    /// Pseudonymous rider id (never a name or address).
    pub rider: String,
    pub meters: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payout {
    pub ride_id: String,
    pub rider: String,
    pub asked_zat: u64,
    pub paid_zat: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochPlan {
    pub budget_zat: u64,
    pub asked_zat: u64,
    pub paid_zat: u64,
    /// Parts per million each ask was paid at (1_000_000 = paid in full).
    pub scale_ppm: u64,
    /// Budget left unpaid; it rolls into the next epoch.
    pub carry_out_zat: u64,
    pub payouts: Vec<Payout>,
}

/// Budget for an epoch: the riders' share of what was mined last epoch, plus
/// whatever went unpaid before.
pub fn epoch_budget(policy: &Policy, mined_last_epoch_zat: u64, carry_in_zat: u64) -> u64 {
    policy.rider_share(mined_last_epoch_zat).saturating_add(carry_in_zat)
}

/// What a ride asks for, before any scaling and before the per-rider cap.
pub fn ask(policy: &Policy, meters: u64) -> u64 {
    let raw = (meters as u128 * policy.base_rate_zat_per_km as u128 / 1000) as u64;
    raw.min(policy.max_zat_per_ride)
}

/// Turns a budget and the epoch's verified rides into payouts.
/// Rides are processed in the order given (the order they were verified).
pub fn plan_epoch(policy: &Policy, budget_zat: u64, rides: &[Ride]) -> EpochPlan {
    let mut used: BTreeMap<&str, u64> = BTreeMap::new();
    let mut asks = Vec::with_capacity(rides.len());
    for r in rides {
        let left = policy
            .max_zat_per_rider_epoch
            .saturating_sub(*used.get(r.rider.as_str()).unwrap_or(&0));
        let a = ask(policy, r.meters).min(left);
        *used.entry(r.rider.as_str()).or_default() += a;
        asks.push(a);
    }
    let asked: u64 = asks.iter().sum();

    let (scale_ppm, payouts): (u64, Vec<Payout>) = if asked <= budget_zat {
        (1_000_000, zip(rides, &asks, |a| a))
    } else {
        let ppm = (budget_zat as u128 * 1_000_000 / asked as u128) as u64;
        (
            ppm,
            zip(rides, &asks, |a| ((a as u128 * budget_zat as u128) / asked as u128) as u64),
        )
    };
    let paid: u64 = payouts.iter().map(|p| p.paid_zat).sum();
    EpochPlan {
        budget_zat,
        asked_zat: asked,
        paid_zat: paid,
        scale_ppm,
        carry_out_zat: budget_zat - paid,
        payouts,
    }
}

fn zip(rides: &[Ride], asks: &[u64], pay: impl Fn(u64) -> u64) -> Vec<Payout> {
    rides
        .iter()
        .zip(asks)
        .map(|(r, &a)| Payout {
            ride_id: r.id.clone(),
            rider: r.rider.clone(),
            asked_zat: a,
            paid_zat: pay(a),
        })
        .collect()
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

    fn ride(id: &str, rider: &str, km: f64) -> Ride {
        Ride { id: id.into(), rider: rider.into(), meters: (km * 1000.0) as u64 }
    }

    #[test]
    fn half_of_mined_becomes_budget() {
        assert_eq!(epoch_budget(&policy(), 1_000_001, 7), 500_000 + 7);
    }

    #[test]
    fn pays_in_full_and_carries_the_rest() {
        let p = plan_epoch(&policy(), 1_000_000, &[ride("a", "r1", 5.0), ride("b", "r2", 10.0)]);
        assert_eq!(p.asked_zat, 150_000);
        assert_eq!(p.paid_zat, 150_000);
        assert_eq!(p.scale_ppm, 1_000_000);
        assert_eq!(p.carry_out_zat, 850_000);
    }

    #[test]
    fn scales_down_when_miners_earned_less_than_asked() {
        let p = plan_epoch(&policy(), 75_000, &[ride("a", "r1", 5.0), ride("b", "r2", 10.0)]);
        assert_eq!(p.scale_ppm, 500_000);
        assert_eq!(p.payouts[0].paid_zat, 25_000);
        assert_eq!(p.payouts[1].paid_zat, 50_000);
        assert!(p.paid_zat <= p.budget_zat);
    }

    #[test]
    fn never_pays_more_than_budget_with_rounding() {
        let rides: Vec<_> = (0..7).map(|i| ride(&i.to_string(), &format!("r{i}"), 3.3)).collect();
        for budget in [1, 2, 99, 12_345, 100_000] {
            let p = plan_epoch(&policy(), budget, &rides);
            assert!(p.paid_zat <= budget, "budget {budget} paid {}", p.paid_zat);
            assert_eq!(p.paid_zat + p.carry_out_zat, budget);
        }
    }

    #[test]
    fn caps_apply_per_ride_and_per_rider() {
        // 80 km asks 800k, capped to 500k per ride; second ride hits the 1M per-rider cap.
        let p = plan_epoch(&policy(), u64::MAX / 2, &[ride("a", "r1", 80.0), ride("b", "r1", 80.0), ride("c", "r1", 80.0)]);
        let asked: Vec<u64> = p.payouts.iter().map(|x| x.asked_zat).collect();
        assert_eq!(asked, vec![500_000, 500_000, 0]);
    }

    #[test]
    fn rejects_bad_policy() {
        let mut p = policy();
        p.rider_share_bps = 10_001;
        assert!(p.validate().is_err());
    }
}
