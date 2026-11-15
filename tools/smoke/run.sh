#!/bin/sh
# Browser smoke tests against a running preview (cd viewer && npx vite preview --port 4789 --strictPort).
# SMOKE_URL overrides the address. Prints the last lines of each script; any "errors: " line other than none fails.
cd "$(dirname "$0")" || exit 2
fail=0
for s in firstvisit deeplink csp mode checkall svpick lakes blocks blocksdeco trees mc oil dimui picker; do
  echo "== $s"
  out=$(timeout 300 node "$s.mjs" 2>&1); rc=$?
  echo "$out" | tail -3
  if [ $rc -ne 0 ] || echo "$out" | grep -E '^errors:? ' | grep -vqE 'none|\[\]'; then fail=1; echo "!! $s failed"; fi
done
exit $fail
