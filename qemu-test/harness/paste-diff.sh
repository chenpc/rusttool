#!/usr/bin/env bash
# Differential test for paste against GNU paste.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=paste
SYS=/usr/bin/paste
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# run_case evaluates SETUP in the same directory it runs COMMAND in, so the
# setup strings below are the fixture writers themselves. "$(printf ...)" is
# used inside them rather than hand-spliced \n, so a missing newline cannot
# hide in the source.
ABC="printf '%s\n' 'a' 'b' 'c' > f; printf '%s\n' 'x' 'y' 'z' > g; printf '%s\n' '1' '2' > h"
BLANKS="printf '%s\n' 'a' '' 'b' '' '' 'c' > f; printf '%s\n' 'p' '' 'q' > g"

run_case "one file"              "$ABC" 'paste f'
run_case "two files"             "$ABC" 'paste f g'
run_case "three files"           "$ABC" 'paste f g h'
run_case "same file twice"       "$ABC" 'paste f f'
run_case "empty file alone"      ': > f' 'paste f'
run_case "empty file last"       "$ABC; : > e" 'paste f g e'
run_case "empty file first"      "$ABC; : > e" 'paste e f'
run_case "all empty"             ': > f; : > g' 'paste f g'
run_case "empty middle"          "$ABC; : > e" 'paste f e g'
run_case "blank lines"           "$BLANKS" 'paste f g'
run_case "no trailing newline"   'printf "a\nb" > f; printf "1" > g' 'paste f g'
run_case "no trailing last"      'printf "a\nb" > f; printf "1\n2" > g' 'paste f g'
run_case "no trailing first"     'printf "a" > f; printf "1\n2" > g' 'paste f g'
run_case "no trailing serial"    'printf "a\nb" > f' 'paste -s f'
run_case "single byte files"     'printf a > f; printf b > g' 'paste f g'
run_case "spaces kept"           'printf "p q\nr s\n" > f; printf "1 2\n" > g' 'paste f g'

run_case "serial one"            "$ABC" 'paste -s f'
run_case "serial two"            "$ABC" 'paste -s f g'
run_case "serial long form"      "$ABC" 'paste --serial f g'
run_case "serial empty"          "$ABC; : > e" 'paste -s f e g'
run_case "serial empty only"     ': > e' 'paste -s e'
run_case "serial blanks"         "$BLANKS" 'paste -s f'
run_case "serial three"          "$ABC" 'paste -s f g h'

run_case "delimiter one"         "$ABC" 'paste -d, f g'
run_case "delimiter two"         "$ABC" 'paste -d xy f g'
run_case "delimiter three"       "$ABC" 'paste -dxyz f g h'
run_case "delimiter cycles"      "$ABC" 'paste -dxy f g h'
run_case "delimiter short last"  "$ABC; printf '1\n' > e" 'paste -dxy f g e'
run_case "delimiter short mid"   "$ABC; printf '1\n2\n' > e" 'paste -dxy f e g'
run_case "delimiter long"        "$ABC" 'paste -dqwerty f g'
run_case "delimiter long form"   "$ABC" 'paste --delimiters=xy f g'
run_case "delimiter long space"  "$ABC" 'paste --delimiters xy f g'
run_case "delimiter attached"    "$ABC" 'paste -dxy f g'
run_case "delimiter next arg"    "$ABC" 'paste -d xy f g'
run_case "delimiter empty"       "$ABC" 'paste -d "" f g'
run_case "delimiter empty form"  "$ABC" 'paste --delimiters= f g'
run_case "delimiter nul"         "$ABC" 'paste -d "\\0" f g'
run_case "delimiter nul mid"     "$ABC" 'paste -d "x\\0y" f g'
run_case "delimiter nul serial"  "$ABC" 'paste -s -d "\\0" f'
run_case "delimiter tab escape"  "$ABC" 'paste -d "\\t" f g'
run_case "delimiter nl escape"   "$ABC" 'paste -d "\\n" f g'
run_case "delimiter cr escape"   "$ABC" 'paste -d "\\r" f g'
run_case "delimiter bs escape"   "$ABC" 'paste -d "\\b" f g'
run_case "delimiter vt escape"   "$ABC" 'paste -d "\\v" f g'
run_case "delimiter ff escape"   "$ABC" 'paste -d "\\f" f g'
run_case "delimiter a escape"    "$ABC" 'paste -d "\\a" f g'
run_case "delimiter bs pair"     "$ABC" 'paste -d "a\\\\b" f g'
run_case "delimiter unknown"     "$ABC" 'paste -d "\\q" f g'
run_case "delimiter serial"      "$ABC" 'paste -s -dxy f'
run_case "delimiter serial two"  "$ABC" 'paste -s -dxy f g'
run_case "delimiter serial none" "$ABC" 'paste -s -d "" f'
run_case "delimiter last wins"   "$ABC" 'paste -d, -d. f g'

run_case "trailing backslash"    "$ABC" 'paste -d "xy\\" f'
run_case "lone backslash"        "$ABC" 'paste -d "\\" f'
run_case "escaped backslash"     "$ABC" 'paste -d "xy\\\\" f'
run_case "three backslashes"     "$ABC" 'paste -d "xy\\\\\\" f'
run_case "backslash then space"  "$ABC" 'paste -d "a b\\" f'
run_case "backslash then quote"  "$ABC" 'paste -d "a\"b\\" f'
run_case "backslash then apos"   "$ABC" "paste -d \"a'b\\\\\" f"
run_case "backslash then ctrl"   "$ABC" 'paste -d "\001\\" f'
run_case "backslash then colon"  "$ABC" 'paste -d "a:b\\" f'
run_case "backslash then tab"    "$ABC" 'paste -d "$(printf "a\tb\\")" f'
run_case "backslash then nl"     "$ABC" 'paste -d "$(printf "a\nb\\")" f'
run_case "backslash then del"    "$ABC" 'paste -d "$(printf "a\177b\\")" f'

run_case "stdin"                 "$ABC" 'paste < f'
run_case "dash is stdin"         "$ABC" 'paste - < f'
run_case "dash twice"            "$ABC" 'paste - - < f'
run_case "stdin and file"        "$ABC" 'paste - g'
run_case "stdin serial"          "$ABC" 'paste -s < f'
run_case "stdin then dash"       "$ABC" 'paste g -'
run_case "double dash"           "$ABC" 'paste -- f g'

run_case "zero plain"            "$ABC" 'paste -z < f'
run_case "zero serial"           "$ABC" 'paste -zs < f'
run_case "zero long form"        "$ABC" 'paste --zero-terminated < f'
run_case "zero two files"        "$ABC" 'paste -z f g'
run_case "zero short"            "$ABC; printf '1\n2\n' > e" 'paste -z f e g'
run_case "zero plus delimiter"   "$ABC" 'paste -z -d, f g'
run_case "zero bundle"           "$ABC" 'paste -zsd, f g'
run_case "zero no terminator"    'printf "a\nb" > f' 'paste -z -s f'

run_case "missing file"          "$ABC" 'paste nope g'
run_case "missing file serial"   "$ABC" 'paste -s nope g'
run_case "missing file middle"   "$ABC" 'paste f nope g'
run_case "missing only"          "$ABC" 'paste nope'
run_case "missing only serial"   "$ABC" 'paste -s nope'
run_case "missing after dash"    "$ABC" 'paste f - nope'
run_case "missing name space"    "$ABC" 'paste "no pe"'
run_case "missing name apos"     "$ABC" "paste \"no'pe\""
run_case "missing name tab"      "$ABC" 'paste "$(printf "no\tpe")"'
run_case "missing name quote"    "$ABC" 'paste "no\"pe"'
run_case "missing name empty"    "$ABC" 'paste ""'

run_case "bad option"            "$ABC" 'paste -Q f'
run_case "bad long option"       "$ABC" 'paste --nonsense f'
run_case "missing d argument"    "$ABC" 'paste f -d'
run_case "missing long argument" "$ABC" 'paste f --delimiters'
run_case "option bundle"         "$ABC" 'paste -sz f g'
run_case "bad in bundle"         "$ABC" 'paste -sQ f g'

summary