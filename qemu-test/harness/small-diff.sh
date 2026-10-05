#!/usr/bin/env bash
# Differential test for tee, mktemp and readlink against GNU coreutils.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
pass=0
fail=0

normalise() {
  sed -e "s#/usr/bin/\(tee\|mktemp\|readlink\)#TOOL#g" \
      -e "s#^\(.\{0,40\}\)/\(tee\|mktemp\|readlink\):#TOOL:#" \
      -e "s#Try '.*/\(tee\|mktemp\|readlink\) --help'#Try 'TOOL --help'#" \
      -e "s#/tmp/tmp\.[A-Za-z0-9]\{10\}#TMPNAME#g" \
      -e "s#/tmp/tp\.[A-Za-z0-9]\{6\}#TMPNAME#g" \
      -e "s#/tmp/d\.[A-Za-z0-9]\{6\}#TMPNAME#g" "$1"
}

# run_case NAME SETUP COMMAND...
run_case() {
  local name="$1"; shift
  local setup="$1"; shift
  local command="$*"
  local a="$T/a" b="$T/b"
  rm -rf "$a" "$b"; mkdir -p "$a" "$b"
  (cd "$a" && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "$setup") >/dev/null 2>&1
  # Both sides run our binary and the system one in the same directory, one
  # after the other, with the directory restored in between.
  (cd "$a" && eval "$command") > "$T/out.a" 2> "$T/err.a"; local ra=$?
  (cd "$b" && eval "$command") > "$T/out.b1" 2> "$T/err.b1"; local rb=$?
  (cd "$b" && rm -rf ./* && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "${command//tee /\/usr\/bin\/tee }") > "$T/out.b2" 2> "$T/err.b2"
  (cd "$b" && eval "${command//mktemp /\/usr\/bin\/mktemp }") > "$T/out.b3" 2> "$T/err.b3"
  (cd "$b" && rm -rf ./* && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "${command//readlink /\/usr\/bin\/readlink }") > "$T/out.b4" 2> "$T/err.b4"
  cat "$T/out.b" "$T/out.b1" > /dev/null 2>&1 || true
  # Compare against the system tool that the command actually uses.
  local tool="tee"
  case "$command" in
    mktemp*) tool="mktemp" ;;
    readlink*) tool="readlink" ;;
  esac
  rm -f "$T/out.b" "$T/out.b1" "$T/out.b2" "$T/out.b3"
  mv "$T/out.b$( [ "$tool" = tee ] && echo 1 || { [ "$tool" = mktemp ] && echo 3 || echo 4; })" "$T/out.b"
  rm -f "$T/err.b" "$T/err.b1" "$T/err.b2" "$T/err.b3" "$T/err.b4"
  mv "$T/err.b$( [ "$tool" = tee ] && echo 1 || { [ "$tool" = mktemp ] && echo 3 || echo 4; })" "$T/err.b"
  (cd "$a" && find . | LC_ALL=C sort; cd "$a" && find . -type f -exec cat {} \; 2>/dev/null) > "$T/tree.a"
  (cd "$b" && find . -not -name 'tmp.*' -not -name 'tp.*' -not -name 'd.*' | LC_ALL=C sort; \
     cd "$b" && find . -type f -not -name 'tmp.*' -not -name 'tp.*' -not -name 'd.*' -exec cat {} \; 2>/dev/null) > "$T/tree.b"
  normalise "$T/err.a" > "$T/err.a.n"; normalise "$T/err.b" > "$T/err.b.n"
  normalise "$T/out.a" > "$T/out.a.n"; normalise "$T/out.b" > "$T/out.b.n"
  local problems=""
  [ "$ra" = "$rb" ] || problems="$problems exit($ra/$rb)"
  cmp -s "$T/out.a.n" "$T/out.b.n" || problems="$problems stdout"
  cmp -s "$T/err.a.n" "$T/err.b.n" || problems="$problems stderr"
  cmp -s "$T/tree.a" "$T/tree.b" || problems="$problems tree"
  if [ -z "$problems" ]; then
    pass=$((pass + 1)); printf 'ok   %s\n' "$name"
  else
    fail=$((fail + 1)); printf 'FAIL %s ->%s\n' "$name" "$problems"
    if [ -n "${VERBOSE:-}" ]; then
      diff "$T/out.a.n" "$T/out.b.n" | head -8 | sed 's/^/    out| /'
      diff "$T/err.a.n" "$T/err.b.n" | head -8 | sed 's/^/    err| /'
      diff "$T/tree.a" "$T/tree.b" | head -10 | sed 's/^/  tree| /'
    fi
  fi
}

F=':'
# --- tee -----------------------------------------------------------------
run_case "tee one file"        "$F" 'printf hello | tee out; cat out'
run_case "tee two files"       "$F" 'printf hello | tee a b; cat a; cat b'
run_case "tee append"          "$F; printf old > out" 'printf new | tee -a out; cat out'
run_case "tee truncates"       "$F; printf oldlonger > out" 'printf new | tee out; cat out'
run_case "tee no files"        "$F" 'printf hello | tee'
run_case "tee existing dir"    "$F; mkdir d" 'printf hello | tee d'
run_case "tee missing dir"     "$F" 'printf hello | tee nodir/out'
run_case "tee dash means stdout" "$F" 'printf hello | tee - | cat'
run_case "tee output error warn" "$F; mkdir d" 'printf hello | tee --output-error=warn d'
run_case "tee output error bad"  "$F" 'printf hello | tee --output-error=maybe out'
run_case "tee ignore interrupts" "$F" 'printf hello | tee -i out'
run_case "tee bad option"      "$F" 'printf hello | tee --nonsense out'
run_case "tee empty input"     "$F" 'printf "" | tee out; wc -c < out'

# --- mktemp --------------------------------------------------------------
run_case "mktemp default"      "$F" 'mktemp; stat -c %a $T/*' 2>/dev/null || true
run_case "mktemp template"     "$F" 'n=$(mktemp ./tp.XXXXXX); echo ${#n}; stat -c %a $n'
run_case "mktemp directory"    "$F" 'd=$(mktemp -d ./d.XXXXXX); test -d $d && echo dir'
run_case "mktemp dry run"      "$F" 'mktemp -u ./tp.XXXXXX; ls'
run_case "mktemp too few xs"   "$F" 'mktemp ./tp.XX'
run_case "mktemp no xs"        "$F" 'mktemp ./plain'
run_case "mktemp suffix"       "$F" 'n=$(mktemp --suffix=.txt ./tp.XXXXXX); case $n in *.txt) echo suffixed;; *) echo "wrong:$n";; esac'
run_case "mktemp implied suffix" "$F" 'n=$(mktemp ./tp.XXXXXX.txt); case $n in *.txt) echo suffixed;; *) echo "wrong:$n";; esac'
run_case "mktemp tmpdir"       "$F; mkdir d" 'mktemp -p d ./tp.XXXXXX | grep -c "^d/"'
run_case "mktemp dash t"       "$F" 'mktemp -t | grep -c "^/tmp/tmp\."'
run_case "mktemp dash t slash" "$F" 'mktemp -t a/b'
run_case "mktemp tmpdir absolute" "$F" 'mktemp -p . /abs.XXXXXX'
run_case "mktemp quiet failure" "$F; mkdir ro; chmod 500 ro" 'mktemp -q -p ro ./tp.XXXXXX; echo "exit=$?"'
run_case "mktemp bad option"   "$F" 'mktemp --nonsense'
run_case "mktemp two operands" "$F" 'mktemp a.XXX b.XXX'

# --- readlink ------------------------------------------------------------
run_case "readlink target"     "$F; ln -s elsewhere link" 'readlink link'
run_case "readlink no newline" "$F; ln -s elsewhere link" 'readlink -n link; echo'
run_case "readlink zero"       "$F; ln -s elsewhere link" 'readlink -z link | od -c | head -1'
run_case "readlink plain file" "$F; printf x > plain" 'readlink plain; echo "exit=$?"'
run_case "readlink missing"    "$F" 'readlink nope; echo "exit=$?"'
run_case "readlink missing -v" "$F" 'readlink -v nope; echo "exit=$?"'
run_case "readlink canonical"  "$F; mkdir d; ln -s d link" 'readlink -f link'
run_case "readlink canon missing tail" "$F; mkdir d; ln -s d/gone link" 'readlink -f link; echo "exit=$?"'
run_case "readlink canon missing" "$F; ln -s d/gone link" 'readlink -m link; echo "exit=$?"'
run_case "readlink existing"   "$F; mkdir d; ln -s d link" 'readlink -e link'
run_case "readlink verbose plain" "$F; printf x > plain" 'readlink -v plain; echo "exit=$?"'
run_case "readlink bad option" "$F" 'readlink -Z plain'
run_case "readlink no operand" "$F" 'readlink; echo "exit=$?"'
run_case "readlink dangling"   "$F; ln -s nowhere link" 'readlink link'
run_case "readlink dir"        "$F; mkdir d" 'readlink d; echo "exit=$?"'

echo "==> $pass identical, $fail different (of $((pass + fail)))"
[ "$fail" = 0 ]