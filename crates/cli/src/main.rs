//! `minesplit` CLI.

mod pay;
mod wallet;

use std::collections::{BTreeMap, HashSet};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use minesplit_policy::Policy;
use minesplit_statement::{build, check, check_public, HeightRange, Inputs, MinedItem, PaidItem, PublicStatement, SplitStatement};
use zcash_address::unified::{self, Container, Encoding};

#[derive(Parser)]
#[command(name = "minesplit", about = "Shielded payouts with a provable, leak-free promise")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Reduce a unified address to its Orchard receiver only.
    OrchardOnly { ua: String },
    /// Build this epoch's single batched shielded payment (ZIP 321 URI).
    Pay {
        #[arg(long)]
        db: String,
        #[arg(long)]
        policy: String,
        #[arg(long)]
        claims: String,
        /// Income window, FROM:TO block heights.
        #[arg(long)]
        income: String,
        #[arg(long, default_value_t = 0)]
        carry_in: u64,
    },
    /// Auditor: build the private statement from the wallet and publish the
    /// totals-only public statement.
    Audit {
        #[arg(long)]
        db: String,
        #[arg(long)]
        policy: String,
        #[arg(long)]
        label: String,
        #[arg(long)]
        network: String,
        #[arg(long)]
        epoch: u64,
        #[arg(long)]
        income: String,
        #[arg(long)]
        payouts: String,
        /// File holding the treasury's UFVK; only its sha256 is published.
        #[arg(long)]
        ufvk_file: String,
        /// Previous epoch's PRIVATE statement (auditor-only).
        #[arg(long)]
        prev: Option<String>,
        #[arg(long)]
        out_private: String,
        #[arg(long)]
        out_public: String,
    },
    /// Check our own footprint for leaks. Exits non-zero on any failure.
    LeakReport {
        #[arg(long)]
        db: String,
        /// Public statements to scan for per-payout data.
        public: Vec<String>,
    },
    /// Verify a chain of public statements, oldest first.
    VerifyPublic { files: Vec<String> },
}

fn range(s: &str) -> Result<HeightRange> {
    let (a, b) = s.split_once(':').context("range must be FROM:TO")?;
    Ok(HeightRange { from: a.parse()?, to: b.parse()? })
}

fn read_json<T: serde::de::DeserializeOwned>(f: &str) -> Result<T> {
    serde_json::from_str(&std::fs::read_to_string(f).with_context(|| format!("reading {f}"))?)
        .with_context(|| format!("parsing {f}"))
}

fn write_json<T: serde::Serialize>(f: &str, v: &T) -> Result<()> {
    if let Some(d) = std::path::Path::new(f).parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(f, serde_json::to_string_pretty(v)?)?;
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::OrchardOnly { ua } => println!("{}", orchard_only(&ua)?),

        Cmd::Pay { db, policy, claims, income, carry_in } => {
            let policy: Policy = read_json(&policy)?;
            let claims: Vec<pay::Claim> = read_json(&claims)?;
            let r = range(&income)?;
            let outs = wallet::outputs(&wallet::open(&db)?)?;
            let income_zat: u64 = outs
                .iter()
                .filter(|o| o.is_income() && o.height.map_or(false, |h| r.contains(h)))
                .map(|o| o.zat)
                .sum();
            let used: HashSet<String> = outs.iter().filter(|o| o.is_payout()).filter_map(|o| o.to_address.clone()).collect();
            let budget = policy.rider_share(income_zat) + carry_in;
            let b = pay::build(&policy, budget, &claims, &used)?;
            eprintln!(
                "income {income_zat} zat → budget {budget} zat · asked {} · paying {} in ONE tx to {} fresh addresses · carry {}",
                b.plan.asked_zat, b.plan.paid_zat, b.payments, b.plan.carry_out_zat
            );
            println!("{}", b.uri);
        }

        Cmd::Audit { db, policy, label, network, epoch, income, payouts, ufvk_file, prev, out_private, out_public } => {
            let policy: Policy = read_json(&policy)?;
            let (ir, pr) = (range(&income)?, range(&payouts)?);
            let outs = wallet::outputs(&wallet::open(&db)?)?;
            if let Some(t) = outs.iter().find(|o| o.pool == wallet::POOL_TRANSPARENT) {
                bail!("transparent output in tx {}: refusing to audit a treasury that touched the transparent pool", t.txid);
            }
            let mined: Vec<MinedItem> = outs
                .iter()
                .filter(|o| o.is_income())
                .filter_map(|o| o.height.filter(|h| ir.contains(*h)).map(|h| MinedItem { txid: format!("{}:{}", o.txid, o.zat), height: h, zat: o.zat }))
                .collect();
            let paid: Vec<PaidItem> = outs
                .iter()
                .filter(|o| o.is_payout())
                .filter_map(|o| o.height.filter(|h| pr.contains(*h)).map(|h| PaidItem { txid: format!("{}:{}", o.txid, o.to_address.clone().unwrap_or_default()), height: h, zat: o.zat }))
                .collect();
            let prev_s: Option<SplitStatement> = prev.as_deref().map(read_json).transpose()?;
            let ufvk = std::fs::read_to_string(&ufvk_file)?;
            let treasury = hex::encode(<sha2_hash::Sha256 as sha2_hash::Digest>::digest(ufvk.trim().as_bytes()));
            let s = build(Inputs {
                label: &label,
                network: &network,
                epoch,
                prev: prev_s.as_ref(),
                policy: &policy,
                treasury: &treasury,
                income_range: ir,
                payout_range: pr,
                mined,
                paid,
            })?;
            let public = s.public()?;
            write_json(&out_private, &s)?;
            write_json(&out_public, &public)?;
            println!(
                "✓ epoch {epoch}: income {} · budget {} · paid {} in {} payouts · carry {}",
                s.mined_zat, s.budget_zat, s.paid_zat, s.paid.len(), s.carry_out_zat
            );
            println!("  private (auditor only): {out_private}");
            println!("  public (publish this):  {out_public}");
        }

        Cmd::LeakReport { db, public } => {
            let outs = wallet::outputs(&wallet::open(&db)?)?;
            let mut rows: Vec<(bool, String, String)> = vec![];
            let transparent = outs.iter().filter(|o| o.pool == wallet::POOL_TRANSPARENT).count();
            rows.push((transparent == 0, "transparent pool".into(), format!("{transparent} transparent outputs")));
            let pays: Vec<_> = outs.iter().filter(|o| o.is_payout()).collect();
            let non_orchard = pays.iter().filter(|o| !wallet::is_orchard_protocol(o.pool)).count();
            rows.push((non_orchard == 0, "payouts Orchard/Ironwood".into(), format!("{non_orchard} of {} in another pool", pays.len())));
            let mut seen = HashSet::new();
            let reused = pays.iter().filter_map(|o| o.to_address.as_ref()).filter(|a| !seen.insert(a.as_str())).count();
            rows.push((reused == 0, "address reuse".into(), format!("{reused} reused payout addresses")));
            let dirty = pays
                .iter()
                .filter(|o| {
                    let m = o.memo.as_deref().map(memo_text).unwrap_or_default();
                    !m.is_empty() && !pay::memo_is_clean(&m)
                })
                .count();
            rows.push((dirty == 0, "memos".into(), format!("{dirty} memos carry more than a receipt id")));
            let mut by_tx: BTreeMap<&str, usize> = BTreeMap::new();
            for o in &pays {
                *by_tx.entry(o.txid.as_str()).or_default() += 1;
            }
            let singles = by_tx.values().filter(|&&n| n < pay_min()).count();
            rows.push((singles == 0, "batched payouts".into(), format!("{} payout txs, {singles} with fewer than {} payouts", by_tx.len(), pay_min())));
            for f in &public {
                let text = std::fs::read_to_string(f)?;
                let leaked = pays.iter().any(|o| {
                    text.contains(&o.txid) || o.to_address.as_ref().map_or(false, |a| text.contains(a.as_str()))
                }) || outs.iter().filter(|o| o.is_income()).any(|o| text.contains(&o.txid));
                rows.push((!leaked, format!("public file {f}"), if leaked { "contains a txid or address".into() } else { "totals only".into() }));
            }
            let mut ok = true;
            println!("\n  MINESPLIT LEAK REPORT\n");
            for (pass, what, detail) in &rows {
                ok &= *pass;
                println!("  {} {:<28} {}", if *pass { "✓" } else { "✗" }, what, detail);
            }
            println!("\n  not checked here: network metadata (use --connection tor or your own node)\n");
            if !ok {
                bail!("leak report failed");
            }
        }

        Cmd::VerifyPublic { files } => {
            let mut prev: Option<PublicStatement> = None;
            let mut ok = true;
            for f in &files {
                let s: PublicStatement = read_json(f)?;
                let bad = check_public(&s, prev.as_ref());
                if bad.is_empty() {
                    println!("✓ epoch {}: income {} · paid {} of budget {} zat · carried {}", s.epoch, s.mined_zat, s.paid_zat, s.budget_zat, s.carry_out_zat);
                } else {
                    ok = false;
                    println!("✗ epoch {}: {}", s.epoch, bad.join("; "));
                }
                prev = Some(s);
            }
            if !ok {
                bail!("public chain does not verify");
            }
        }
    }
    let _ = check; // re-exported for library users
    Ok(())
}

fn pay_min() -> usize {
    minesplit_statement::MIN_PUBLIC_PAYOUTS
}

fn memo_text(b: &[u8]) -> String {
    // ZIP 302: a text memo is UTF-8 padded with zeros; 0xF6 marks "no memo".
    if b.first() == Some(&0xF6) {
        return String::new();
    }
    String::from_utf8_lossy(b).trim_end_matches('\0').to_string()
}

pub fn orchard_only(ua: &str) -> Result<String> {
    let (net, addr) = unified::Address::decode(ua).context("not a unified address")?;
    let orchard: Vec<unified::Receiver> = addr
        .items()
        .into_iter()
        .filter(|r| matches!(r, unified::Receiver::Orchard(_)))
        .collect();
    if orchard.is_empty() {
        bail!("this address has no Orchard receiver; refusing to use a transparent or Sapling-only address");
    }
    Ok(unified::Address::try_from_items(orchard)?.encode(&net))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_protocol::consensus::NetworkType;

    #[test]
    fn strips_everything_but_orchard() {
        let full = unified::Address::try_from_items(vec![unified::Receiver::P2pkh([7; 20]), unified::Receiver::Orchard([9; 43])])
            .unwrap()
            .encode(&NetworkType::Test);
        let only = orchard_only(&full).unwrap();
        let (_, a) = unified::Address::decode(&only).unwrap();
        assert!(a.items().iter().all(|r| matches!(r, unified::Receiver::Orchard(_))));
    }

    #[test]
    fn refuses_non_orchard() {
        let s = unified::Address::try_from_items(vec![unified::Receiver::Sapling([3; 43])]).unwrap().encode(&NetworkType::Test);
        assert!(orchard_only(&s).is_err());
    }
}
