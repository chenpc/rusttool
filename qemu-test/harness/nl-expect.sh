#!/usr/bin/env bash
# Generate every expected value for the nl guest case from /usr/bin/nl itself.
set -eu

W=/tmp/opencode/nl-case
rm -rf "$W"
mkdir -p "$W"

printf '%s\n' 'H1' '\:' '' 'B1' 'B2' '\:\:' '' 'B3' '\:\:\:' 'F1' '' 'F2' > "$W/pages"
printf '%s\n' 'a' '' 'b' '' '' 'c' > "$W/tabs"
printf '%s\n' 'x y' 'p 12' 'r s' 'q 7' > "$W/words"

emit() {
  local label=$1
  shift
  printf '=== %s :: nl %s\n' "$label" "$*"
  local out err code
  out=$("$@" 2>"$W/.err") && code=0 || code=$?
  err=$(cat "$W/.err")
  printf 'exit=%s\n' "$code"
  printf 'out=%q\n' "$out"
  [ -n "$err" ] && printf 'err=%q\n' "$err"
  printf '\n'
}

emit nl-basic            nl -ba "$W/tabs"
emit nl-default          nl "$W/pages"
emit nl-default-tabs     nl "$W/tabs"
emit nl-no-number        nl -bn "$W/pages"
emit nl-header-footer    nl -h a -f a "$W/pages"
emit nl-header-t         nl -h t "$W/pages"
emit nl-footer-t         nl -f t "$W/pages"
emit nl-no-renumber      nl -ba -p "$W/pages"
emit nl-start-0          nl -ba -v0 "$W/pages"
emit nl-start-negative   nl -ba -v-3 "$W/tabs"
emit nl-increment-2      nl -ba -i2 "$W/tabs"
emit nl-increment-neg    nl -ba -i-1 "$W/tabs"
emit nl-width-3          nl -ba -w3 "$W/tabs"
emit nl-width-1          nl -ba -w1 "$W/tabs"
emit nl-format-left      nl -ba -nln -w4 "$W/tabs"
emit nl-format-zero      nl -ba -nrz -w4 "$W/tabs"
emit nl-format-zero-neg  nl -ba -nrz -v-2 -w5 "$W/tabs"
emit nl-sep-dots         nl -ba -s. -w3 "$W/tabs"
emit nl-sep-empty        nl -w3 "$W/tabs" -s
emit nl-sep-many         nl -ba -w3 -s' | ' "$W/tabs"
emit nl-sep-tab-again    nl -ba -w3 -s$'\t' "$W/tabs"
emit nl-join-2           nl -ba -l2 "$W/tabs"
emit nl-join-3           nl -ba -l3 "$W/tabs"
emit nl-join-style-t     nl -bt -l2 "$W/tabs"
emit nl-delim-colon      nl -ba -d: "$W/pages"
emit nl-delim-ab         nl -ba -dab "$W/pages"
emit nl-delim-empty      nl -ba -d '' "$W/pages"
emit nl-pat-digit        nl -bp'[0-9]' "$W/words"
emit nl-pat-none         nl -bpzzz "$W/words"
emit nl-pat-anchor       nl -bp'^p' "$W/words"
emit nl-pat-bracket      nl -bp'[xy]' "$W/words"
emit nl-pat-group        nl -bp'\(x\|y\) ' "$W/words"
emit nl-pat-star         nl -bp'q*' "$W/words"
emit nl-pat-header       nl -bpn -hp'^H' "$W/pages"
emit nl-pat-footer       nl -bpn -fp'^F' "$W/pages"
emit nl-pat-dot          nl -bp'..' "$W/words"
emit nl-long-form        nl --body-numbering=a --number-width=3 --number-separator=. "$W/tabs"
emit nl-two-files        nl -ba "$W/tabs" "$W/words"
emit nl-stdin            sh -c 'nl -ba < /tmp/opencode/nl-case/tabs'
printf 'no trailing newline\n' > "$W/nonl"
emit nl-no-trailing-nl   nl -ba "$W/nonl"
emit nl-bad-style        nl -bz "$W/tabs"
emit nl-bad-width        nl -wx "$W/tabs"
emit nl-bad-regex        nl -bp'[' "$W/tabs"
emit nl-missing-arg      nl -w
emit nl-bad-option       nl -Z
emit nl-missing-file     nl /nonexistent/nope