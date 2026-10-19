# minesplit

**Mined ZEC, split in the open, paid in private.**

Shielded payouts with a promise anyone can check, and nothing that leaks.

A treasury (funded by mining, grants, or donations) publishes a rule: a fixed
share of everything it receives goes to the people it pays. Income arrives
shielded, payouts go out shielded, and each epoch the treasury publishes a
**public statement** of totals that proves the rule held:

```
budget  = share(income) + carry_in
budget == paid + carry_out                  nothing leaks out
carry_in == previous carry_out              nothing disappears between epochs
```

An auditor the treasury chooses checks every item behind those totals with the
treasury's viewing keys and signs the digest. The public sees totals, the hash
chain and the signature, and nothing per payout.

## No leaks: the rules this repo enforces

| Leak | How it is closed | Enforced by |
|---|---|---|
| Transparent addresses | None anywhere; payout addresses are reduced to Orchard-only | `minesplit orchard-only`, tests |
| Per-payout txids, amounts or addresses in public | Public statement is totals only | `public_view_leaks_nothing_per_payout` test |
| Small epochs exposing one person's pay | Totals are refused below 5 payouts; merge epochs | `small_epochs_are_not_published` test |
| Payout timing | One batched payout per epoch at a fixed time | payout runner (next) |
| Address reuse | Fresh address per payout | payout runner (next) |
| Memos | Random receipt id only | memo builder (next) |
| Keys or seeds in git | Pre-commit hook blocks wallet files, keys, seed-like lines | `scripts/check-secrets.sh` |
| Network metadata (IP, which txids you fetch) | Not closed yet: use your own node or Tor | stated limit |

Built by Intelligrip Inc. for [Pedalshield](https://pedalshield.app) (get paid
to ride your bike, privately). ZECATHON 2026, Shielded Payments track.

## Status

| Part | State |
|---|---|
| `policy`: split, per-km asks, caps, pro-rata scaling, carry | done, tested |
| `statement`: auditor view, totals-only public view, hash chain, leak tests | done, tested |
| `minesplit` CLI: Orchard-only addresses, verify public chain | done, tested |
| Treasury wallet on testnet (zcash-devtool) | next |
| Pay: shielded rider payouts with memo receipts | next (zcash-devtool / librustzcash) |
| Chain checks + sender-side payout proofs (OVK) | next, written fresh here |
| Dashboard: latest verified statement | next |

## Honest limits

* A split statement proves the money moved as the policy says. It does not
  prove each recipient is a distinct real rider; the treasury could pay itself.
  Ride attestations and hardware-signed rides (IntelliGrip) narrow that gap.
* Pool payouts to a transparent address are public by design; that is what
  makes income checkable. Everything after the sweep is shielded.

## Build-window rules

All code in this repo is written during the ZECATHON build window. It depends
only on public libraries and tools (librustzcash crates, Zebra,
zcash-devtool); it does not import code from Pedalshield or any other existing
product. Pedalshield is the first intended user, not a dependency.

## Test

```
cargo test
```

License: MIT.
