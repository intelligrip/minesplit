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
| Payout timing | One batched payout per epoch | `minesplit pay`, leak report |
| Address reuse | Fresh random-diversifier Orchard address per payout; reuse refused | `minesplit fresh-addresses`, `pay`, leak report |
| Memos | Random receipt id only | `pay`, leak report |
| Keys or seeds in git | Pre-commit hook blocks wallet files, keys, seed-like lines | `scripts/check-secrets.sh` |
| Network metadata (your IP) | Every wallet call goes over Tor (`--connection tor`) | `scripts/demo.sh` |
| Network metadata (which data you fetch) | Wallets scan compact blocks; the server still serves them. Next: own Zebra node | stated limit |

Built by Intelligrip Inc. for [Pedalshield](https://pedalshield.app) (get paid
to ride your bike, privately). ZECATHON 2026, Shielded Payments track.

## Live on Zcash mainnet

Epoch 0 ran on mainnet on Oct 28, 2026, every network call over Tor:

| | |
|---|---|
| Income | 0.01 ZEC, received shielded (Ironwood pool) |
| Policy | 50% of income owed to recipients |
| Claims | 6 rides asked for 0.00713 ZEC, more than the 0.005 budget, so all were scaled to 70.1% |
| Payout | **one** shielded transaction to **6 fresh Orchard-only addresses**, 0.00499996 ZEC, memos carry only a random receipt id |
| Carried forward | 4 zat (rounding), owed to recipients next epoch |
| Leak report | 6/6 ✓: no transparent outputs, no address reuse, clean memos, batched, public file totals-only |

The public statement is [`statements/epoch-0.json`](statements/epoch-0.json). Verify it with no keys:

```
cargo build --release -p minesplit
./target/release/minesplit verify-public statements/epoch-0.json
```

Change one number in it and verification fails. Payout txids and addresses are deliberately not published; the auditor holds them.

## Status

| Part | State |
|---|---|
| `policy`: split, per-km asks, caps, pro-rata scaling, carry | done, tested |
| `statement`: auditor view, totals-only public view, hash chain, leak tests | done, tested |
| `minesplit` CLI: Orchard-only addresses, verify public chain | done, tested |
| Treasury + recipient wallets (zcash-devtool, over Tor) | done, mainnet |
| `pay`: one batched shielded payout, fresh Orchard-only addresses from the recipient's viewing key, receipt-id memos | done, mainnet |
| `audit`: private statement from the wallet's viewing data, totals-only public statement | done, mainnet |
| `leak-report`: checks our own footprint | done, mainnet |
| Auditor signature on the digest; own Zebra node | next |

## Run it

Needs Rust and [zcash-devtool](https://github.com/zcash/zcash-devtool). Every network call goes over Tor.

```
cargo test                          # policy, statements, leak tests
NET=main ./scripts/demo.sh setup    # treasury + recipients wallets (gitignored)
NET=main ./scripts/demo.sh address  # Orchard-only address: send income from a shielded wallet
NET=main ./scripts/demo.sh sync     # after the income confirms
NET=main ./scripts/demo.sh pay      # ONE batched shielded payout, fresh address per recipient
NET=main ./scripts/demo.sh sync
NET=main ./scripts/demo.sh audit    # private statement (auditor) + public statement (totals only)
NET=main ./scripts/demo.sh check    # leak report, verify the public chain, tamper test
```

Anyone can verify the published chain without any keys:

```
./target/release/minesplit verify-public statements/epoch-*.json
```

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
