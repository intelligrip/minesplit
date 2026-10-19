#!/usr/bin/env bash
# Refuses a commit that would publish wallet secrets. Install once:
#   ln -sf ../../scripts/check-secrets.sh .git/hooks/pre-commit
set -euo pipefail
bad=0
files=$(git diff --cached --name-only --diff-filter=ACM)
for f in $files; do
  case "$f" in
    wallets/*|*.identity.txt|*.local|*.sqlite) echo "✗ $f: wallet/key file must never be committed"; bad=1; continue;;
  esac
  [ -f "$f" ] || continue
  [ "$f" = scripts/check-secrets.sh ] && continue
  if grep -En 'secret-extended-key|AGE-SECRET-KEY|uviewtest1|uview1|zxviews|-----BEGIN [A-Z ]*PRIVATE KEY' "$f" >/dev/null; then
    echo "✗ $f: contains a key or viewing key"; bad=1
  fi
  # 24 lowercase words on one line looks like a seed phrase.
  if grep -Eqn '^([a-z]{3,8} ){23}[a-z]{3,8}$' "$f"; then
    echo "✗ $f: line looks like a 24-word seed phrase"; bad=1
  fi
done
[ $bad -eq 0 ] || { echo "commit blocked: secrets would leak"; exit 1; }
