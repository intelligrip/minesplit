# minesplit · ZECATHON 2026 · Shielded Payments

**Shielded payouts with a promise anyone can check, and nothing that leaks.**

Repo: https://github.com/intelligrip/minesplit · Built by Intelligrip Inc. (Samuel B. Newman, Bend, OR)

## One-liner

A treasury pays people in shielded ZEC and publishes a statement anyone can verify: it paid exactly the share it promised, and not one recipient, amount or address is revealed.

## The problem

Anyone who pays many people (grant programs, bounty boards, mining pools, DAOs, apps that reward users) faces a choice. Pay transparently, and every recipient's income is public forever. Pay shielded, and nobody can check that the payer kept its promise. Today you pick privacy or accountability.

## What minesplit does

1. **Publishes a rule.** For example: 50% of everything the treasury receives goes to the people it pays.
2. **Receives shielded.** Income (mining rewards, grants, donations) arrives at an Orchard-only address. No transparent pool, ever.
3. **Pays in one batch.** Each epoch's claims become one shielded transaction. Every recipient gets a fresh, random-diversifier Orchard address derived from their own viewing key, and a memo that is only a random receipt id. If claims exceed the budget, everyone is scaled down by the same factor, so the treasury never promises more than it received.
4. **Proves it.** An auditor with the treasury's viewing key builds the full statement. The public gets a totals-only statement: income, budget, paid, carried forward, hash-chained to the previous epoch. Anyone can run `minesplit verify-public` with no keys. Skim one zatoshi and it fails.
5. **Checks itself for leaks.** `minesplit leak-report` audits our own on-chain footprint: transparent use, pools, address reuse, memo contents, batching, and whether any public file contains a txid or address.

## It runs on mainnet

Epoch 0, Oct 28, 2026, every network call over Tor:

- 0.01 ZEC received shielded (Ironwood pool)
- 6 claims asked 0.00713 ZEC against a 0.005 budget, so all were scaled to 70.1%
- **one** shielded transaction paid **6 fresh Orchard-only addresses** 0.00499996 ZEC; 4 zat carried forward
- leak report: 6 of 6 checks pass
- public statement: `statements/epoch-0.json`, verifies; tampered copy fails

## Privacy (no leaks)

| Leak | Closed by |
|---|---|
| Transparent addresses | None anywhere; addresses reduced to Orchard-only |
| Per-payout data in public | Public statement is totals only; a test fails the build otherwise |
| One person's pay inferable from a small epoch | Totals refused below 5 payouts |
| Timing | One batched payout per epoch |
| Address reuse | Fresh random-diversifier address per payout; reuse refused |
| Memos | Random receipt id only, checked |
| Keys in git | Pre-commit hook blocks wallet files, keys, seed-like lines |
| Your IP | All wallet traffic over Tor |
| Remaining limit, stated | The lightwalletd server serves the compact blocks you scan; next step is our own Zebra node |

Disclosed notes cannot be linked to their later spends: that needs the recipient's key.

## Usefulness (Monday)

- **Grant programs and bounty boards:** pay contributors shielded, publish provable totals.
- **Mining pools:** pay miners shielded and prove the promised share was paid.
- **Apps that reward users:** Intelligrip's bike app Pedalshield will use it to pay riders from a miner-funded treasury (Pedalshield is not part of this submission).

## Originality

All code written during the build window. Depends only on public libraries and tools: librustzcash crates, orchard, zcash-devtool, Arti (Tor). No code from Pedalshield or any other existing product.

## Verify it yourself (2 minutes)

```
git clone https://github.com/intelligrip/minesplit && cd minesplit
cargo test                                             # 19 tests incl. leak tests
cargo build --release -p minesplit
./target/release/minesplit verify-public statements/epoch-0.json
```

## Honest limits

- Proves the money moved as the rule says, not that each recipient is a distinct person.
- The public trusts the auditor's check of items behind the totals (signature on the digest is next).
- Compact blocks still come from a lightwalletd server, over Tor.

## Demo video (about 2 minutes)

| t | Screen | Say |
|---|---|---|
| 0:00 | Title | "If you pay people in public, their income is public forever. If you pay them privately, nobody can check you kept your promise. minesplit gives you both." |
| 0:15 | `demo/policy.json` | "The treasury publishes a rule: half of everything it receives goes to the people it pays." |
| 0:30 | Terminal: `demo.sh sync` balance | "Income arrives shielded, over Tor. No transparent address anywhere." |
| 0:45 | Terminal: `demo.sh pay` proposal | "Six claims ask for more than the budget, so everyone is scaled down equally. One transaction, six fresh addresses, memos that are just a random receipt id." |
| 1:10 | Terminal: `demo.sh audit` + `cat statements/epoch-0.json` | "The auditor sees every item. The public sees totals: received, paid, carried forward." |
| 1:30 | Terminal: `demo.sh check` | "We check ourselves for leaks: all pass. Anyone can verify the statement. Skim one zatoshi and it fails." |
| 1:50 | README | "Live on mainnet. Built this month. Open source. Pay people privately, and prove it." |
