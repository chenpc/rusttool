#!/usr/bin/env bash
# Differential test for mktemp. The generated name is random on both sides, so
# the normaliser replaces it, and the mode of what was created is compared.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=mktemp
SYS=/usr/bin/mktemp
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# The generated name is random on both sides, so collapse every name that came
# out of a template into a fixed token before comparing.
normalise() {
  sed -e "s#$SYS#$TOOL#g" -e "s#^\(.\{0,40\}\)/$TOOL:#$TOOL:#" \
      -e "s#Try '.*/$TOOL --help'#Try '$TOOL --help'#" \
      -e 's#\(/tmp/tmp\.\)[A-Za-z0-9]\+#\1RAND#g' \
      -e 's#\(tp\.\)[A-Za-z0-9]\+#\1RAND#g' \
      -e 's#\(d\.\)[A-Za-z0-9]\+#\1RAND#g' "$1"
}

# Every created name has the shape the template asks for; compare the shape and
# the kind and mode of the thing that was created.
snapshot() {
  local dir="$1"
  (cd "$dir" && find . -not -name '.*' -printf '%y %m\n' | LC_ALL=C sort)
}

F=':'
run_case "default template"     "$F" 'mktemp | grep -c "^/tmp/tmp\." ; stat -c %a $(mktemp)'
run_case "explicit template"    "$F" 'mktemp ./tp.XXXXXX | grep -c "^\./tp\."; stat -c %a tp.*'
run_case "three xs"             "$F" 'mktemp ./tp.XXX; stat -c %a tp.*'
run_case "ten xs"               "$F" 'mktemp ./tp.XXXXXXXXXX | grep -c "tp\."'
run_case "twenty xs"            "$F" 'mktemp ./tp.XXXXXXXXXXXXXXXXXXXX | grep -c "^\./tp\."'
run_case "directory mode"       "$F" 'd=$(mktemp -d ./d.XXXXXX); test -d $d && stat -c %a $d'
run_case "dry run creates none" "$F" 'mktemp -u ./tp.XXXXXX | grep -c "^\./tp\."; ls | wc -l'
run_case "dry run directory"    "$F" 'mktemp -u -d ./d.XXXXXX | grep -c "^\./d\."; ls | wc -l'
run_case "too few xs"           "$F" 'mktemp ./tp.XX'
run_case "no xs"                "$F" 'mktemp ./plain'
run_case "explicit suffix"      "$F" 'n=$(mktemp --suffix=.txt ./tp.XXXXXX); case $n in *.txt) echo ok;; *) echo "bad:$n";; esac'
run_case "implied suffix"       "$F" 'n=$(mktemp ./tp.XXXXXX.txt); case $n in *.txt) echo ok;; *) echo "bad:$n";; esac'
run_case "suffix option plus X" "$F" 'n=$(mktemp --suffix=.log ./tp.XXXXXX); case $n in *.log) echo ok;; *) echo "bad:$n";; esac'
run_case "suffix with slash"    "$F" 'mktemp --suffix=a/b ./tp.XXXXXX'
run_case "tmpdir relative"      "$F; mkdir d" 'mktemp -p d ./tp.XXXXXX | grep -c "^d/tp\."'
run_case "tmpdir bare"          "$F" 'TMPDIR=/tmp mktemp --tmpdir | grep -c "^/tmp/tmp\."'
run_case "tmpdir with path"     "$F" 'TMPDIR=/var/tmp mktemp -p | grep -c "^/var/tmp/tmp\."'
run_case "tmpdir absolute"      "$F" 'mktemp -p . /abs.XXXXXX'
run_case "tmpdir nested"        "$F; mkdir d" 'mktemp -p d a/tp.XXXXXX | grep -c "^d/a/tp\."; ls d/a | grep -c tp'
run_case "dash t"               "$F" 'TMPDIR=/tmp mktemp -t | grep -c "^/tmp/tmp\."'
run_case "dash t with slash"    "$F" 'mktemp -t a/b'
run_case "dash t and p"         "$F" 'mktemp -t -p /tmp ./tp.XXXXXX'
run_case "quiet failure"        "$F; mkdir ro; chmod 500 ro" 'mktemp -q -p ro ./tp.XXXXXX; echo "exit=$?"'
run_case "loud failure"         "$F; mkdir ro; chmod 500 ro" 'mktemp -p ro ./tp.XXXXXX; echo "exit=$?"'
run_case "missing directory"    "$F" 'mktemp -p nodir ./tp.XXXXXX; echo "exit=$?"'
run_case "two operands"         "$F" 'mktemp a.XXX b.XXX'
run_case "bad option"           "$F" 'mktemp --nonsense'
run_case "bad short option"     "$F" 'mktemp -Z'
run_case "unique each time"     "$F" 'a=$(mktemp ./tp.XXXXXX); b=$(mktemp ./tp.XXXXXX); [ "$a" != "$b" ] && echo differ'
run_case "template with prefix" "$F" 'mktemp ./aXXX | grep -c "^\./a"'
run_case "empty file created"   "$F" 'mktemp ./tp.XXXXXX > /dev/null; stat -c %s tp.*'

summary