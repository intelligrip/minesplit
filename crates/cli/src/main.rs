//! `minesplit` CLI.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use minesplit_statement::{check_public, PublicStatement};
use zcash_address::unified::{self, Container, Encoding};

#[derive(Parser)]
#[command(name = "minesplit", about = "Shielded payouts with a provable, leak-free promise")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Reduce a unified address to its Orchard receiver only, so nobody can
    /// pay it (or be paid from it) through the transparent or Sapling pools.
    OrchardOnly { ua: String },
    /// Verify a chain of public statements, oldest first.
    VerifyPublic { files: Vec<String> },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::OrchardOnly { ua } => println!("{}", orchard_only(&ua)?),
        Cmd::VerifyPublic { files } => {
            let mut prev: Option<PublicStatement> = None;
            let mut ok = true;
            for f in &files {
                let s: PublicStatement = serde_json::from_str(&std::fs::read_to_string(f).with_context(|| format!("reading {f}"))?)?;
                let bad = check_public(&s, prev.as_ref());
                if bad.is_empty() {
                    println!("✓ epoch {}: paid {} of budget {} zat, carried {}", s.epoch, s.paid_zat, s.budget_zat, s.carry_out_zat);
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
    Ok(())
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
        let full = unified::Address::try_from_items(vec![
            unified::Receiver::P2pkh([7; 20]),
            unified::Receiver::Orchard([9; 43]),
        ])
        .unwrap()
        .encode(&NetworkType::Test);
        let only = orchard_only(&full).unwrap();
        let (_, a) = unified::Address::decode(&only).unwrap();
        assert!(a.items().iter().all(|r| matches!(r, unified::Receiver::Orchard(_))));
    }

    #[test]
    fn refuses_transparent_only() {
        // A UA must have a shielded receiver to be valid, so build a Sapling-only one.
        let s = unified::Address::try_from_items(vec![unified::Receiver::Sapling([3; 43])])
            .unwrap()
            .encode(&NetworkType::Test);
        assert!(orchard_only(&s).is_err());
    }
}
