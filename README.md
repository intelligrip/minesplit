# minesplit

**Mined ZEC, split in the open, paid in private.**

A miner-funded shielded treasury that proves it kept its promise. Miners pay a
published transparent address. A published policy sends a fixed share of every
mined zatoshi to riders. Riders are paid in shielded ZEC. Each epoch the
treasury publishes a **split statement** that anyone can check against the chain:

```
budget  = rider_share(mined last epoch) + carry_in
budget == paid_to_riders + carry_out        nothing leaks out
carry_in == previous statement's carry_out  nothing disappears between epochs
```

* **Mined income** is public: pool payouts land on the published mining address.
* **Rider payouts** stay shielded. They are proven with sender-side one-note
  disclosures (the treasury's outgoing viewing key), so no rider keys, routes
  or identities are ever involved.
* Statements are hash-chained. Skim the carry, edit an amount, or skip an epoch
  and verification fails.

Built by Intelligrip Inc. for [Pedalshield](https://pedalshield.app) (get paid
to ride your bike, privately). ZECATHON 2026, Shielded Payments track.

## Status

| Part | State |
|---|---|
| `policy`: split, per-km asks, caps, pro-rata scaling, carry | done, tested |
| `statement`: build + verify + hash chain | done, tested |
| Regtest devnet with Zebra's internal miner | config in `devnet/` |
| Sweep: transparent mining payouts → shielded treasury | next |
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
