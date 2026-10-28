#!/usr/bin/env bash
# minesplit end to end, shielded only, over Tor.
#
#   NET=main ./scripts/demo.sh setup      treasury + riders wallets (once)
#   NET=main ./scripts/demo.sh address    treasury's Orchard-only address: fund it from a shielded wallet
#   NET=main ./scripts/demo.sh sync       sync both wallets
#   NET=main ./scripts/demo.sh pay        one batched shielded payout for this epoch's rides
#   NET=main ./scripts/demo.sh audit      auditor: private statement + totals-only public statement
#   NET=main ./scripts/demo.sh check      leak report + public chain verification + tamper test
set -euo pipefail
cd "$(dirname "$0")/.."
NET="${NET:-main}"
DT="zcash-devtool"
MS="./target/release/minesplit"
CONN=(-s zecrocks --connection "${CONN:-tor}")
T="wallets/treasury-$NET"; R="wallets/riders-$NET"
[ -x "$MS" ] || cargo build -q --release -p minesplit
st() { "$DT" wallet -w "$T" "$@"; }
rd() { "$DT" wallet -w "$R" "$@"; }

case "${1:-}" in
  setup)
    mkdir -p wallets
    [ -d "$T" ] || st init --name "minesplit-treasury" -i "$T.identity.txt" -n "$NET" "${CONN[@]}"
    [ -d "$R" ] || rd init --name "minesplit-riders" -i "$R.identity.txt" -n "$NET" "${CONN[@]}"
    st list-accounts | awk '/UFVK/{print $NF}' > "$T.ufvk.local"
    rd list-accounts | awk '/UFVK/{print $NF}' > "$R.ufvk.local"
    echo "✓ wallets ready in wallets/ (gitignored, blocked by the secret hook)"
    ;;
  address)
    UA=$(st list-addresses | grep -m1 -Eo 'u(test)?1[0-9a-z]+')
    echo "Send income here from a SHIELDED wallet (Zashi/Zodl):"
    "$MS" orchard-only "$UA"
    ;;
  sync)
    st sync "${CONN[@]}"; rd sync "${CONN[@]}"; st balance; ;;
  pay)
    INCOME="${INCOME:-0:99999999}"
    # Each rider wallet makes a brand-new address for its payout: no reuse.
    node -e '
      const rides=require("./demo/rides.json");
      const addrs=require("fs").readFileSync(0,"utf8").trim().split("\n");
      console.log(JSON.stringify(rides.map((r,i)=>({...r,address:addrs[i]})),null,2));
    ' < <([ -s "$R.ufvk.local" ] || rd list-accounts | awk '/UFVK/{print $NF}' > "$R.ufvk.local"; "$MS" fresh-addresses --ufvk-file "$R.ufvk.local" --network "$NET" --count 6) > wallets/claims.local.json
    URI=$("$MS" pay --db "$T/data.sqlite" --policy demo/policy.json --claims wallets/claims.local.json --income "$INCOME" --carry-in "${CARRY:-0}")
    st pay -i "$T.identity.txt" --payment-uri "$URI" "${CONN[@]}"
    ;;
  audit)
    "$MS" audit --db "$T/data.sqlite" --policy demo/policy.json --label "minesplit demo ($NET)" --network "$NET" \
      --epoch "${EPOCH:-0}" --income "${INCOME:-0:99999999}" --payouts "${PAYOUTS:-0:99999999}" \
      --ufvk-file "$T.ufvk.local" ${PREV:+--prev "$PREV"} \
      --out-private "wallets/private/epoch-${EPOCH:-0}.json" --out-public "statements/epoch-${EPOCH:-0}.json"
    ;;
  check)
    "$MS" leak-report --db "$T/data.sqlite" statements/*.json
    "$MS" verify-public statements/epoch-*.json
    echo; echo "Tamper test: skim 1 zat of rider money and re-verify"
    python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));d["carry_out_zat"]-=1;json.dump(d,open("/tmp/minesplit-tampered.json","w"))' "$(ls statements/epoch-*.json | tail -1)"
    "$MS" verify-public /tmp/minesplit-tampered.json || echo "✓ tampering caught"
    ;;
  *) sed -n 2,9p "$0"; exit 1;;
esac
