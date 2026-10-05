#!/usr/bin/env bash
# Differential test for comm against GNU comm.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=comm
SYS=/usr/bin/comm
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# Sorted: a b c d e vs b c d f
S1="printf '%s\n' a b c d e > f1; printf '%s\n' b c d f > f2"
S2="printf '%s\n' a b c > g1; printf '%s\n' a b c > g2"
S3="printf '%s\n' a b c > h1; printf '%s\n' a b c d > h2"
S4="printf '%s\n' a a b > i1; printf '%s\n' a > i2"
# Out of order in various ways.
U1="printf '%s\n' b a c > j1; printf '%s\n' a b c > j2"
U2="printf '%s\n' c b > k1; printf '%s\n' c > k2"
U3="printf '%s\n' b a > l1; printf '%s\n' a b c > l2"
U4="printf '%s\n' d c b a > m1; printf '%s\n' a > m2"
U5="printf '%s\n' a ' ' b > n1; printf '%s\n' ' ' a > n2"
U6="printf '%s\n' b a c > o1; printf '%s\n' d c b > o2"
U7="printf '%s\n' b c a > p1; printf '%s\n' b a c > p2"
U8="printf '%s\n' z y x > q1; printf '%s\n' a b > q2"
# A prefix, an empty file and a missing terminator.
P1="printf 'a b\nb\n' > r1; printf 'a\n' > r2"
E1="printf '%s\n' a b c > s1; : > s2"
E2=": > t1; printf '%s\n' a b > t2"
NN="printf 'a\nb' > u1; printf 'b\nc' > u2"
Z1="printf 'a\0b\0' > v1; printf 'b\0c\0' > v2"
SP="printf '%s\n' 'a b' 'b' > w1; printf '%s\n' a > w2"

run_case "two files"                "$S1" 'comm f1 f2'
run_case "identical"                "$S2" 'comm g1 g2'
run_case "right is longer"          "$S3" 'comm h1 h2'
run_case "repeated left"            "$S4" 'comm i1 i2'
run_case "no first"                 "$S1" 'comm -1 f1 f2'
run_case "no second"                "$S1" 'comm -2 f1 f2'
run_case "no third"                 "$S1" 'comm -3 f1 f2'
run_case "no first and second"      "$S1" 'comm -12 f1 f2'
run_case "no first and third"       "$S1" 'comm -13 f1 f2'
run_case "no second and third"      "$S1" 'comm -23 f1 f2'
run_case "no columns at all"        "$S1" 'comm -123 f1 f2'
run_case "separate flags"           "$S1" 'comm -1 -2 -3 f1 f2'
run_case "total"                    "$S1" 'comm --total f1 f2'
run_case "total no first"           "$S1" 'comm --total -1 f1 f2'
run_case "total identical"          "$S2" 'comm --total g1 g2'
run_case "total twice"              "$S1" 'comm --total --total f1 f2'
run_case "delimiter equals"         "$S1" 'comm --output-delimiter=, f1 f2'
run_case "delimiter space arg"      "$S1" 'comm --output-delimiter , f1 f2'
run_case "delimiter empty"          "$S1" 'comm --output-delimiter= f1 f2'
run_case "delimiter twice same"     "$S1" 'comm --output-delimiter=, --output-delimiter=, f1 f2'
run_case "delimiter twice differ"   "$S1" 'comm --output-delimiter=, --output-delimiter=X f1 f2'
run_case "delimiter long"           "$S1" 'comm --output-delimiter=XY f1 f2'
run_case "delimiter with total"     "$S1" 'comm --total --output-delimiter=, f1 f2'
run_case "prefix"                   "$P1" 'comm r1 r2'
run_case "empty right"              "$E1" 'comm s1 s2'
run_case "empty left"               "$E2" 'comm t1 t2'
run_case "both empty"               ': > a; : > b' 'comm a b'
run_case "no trailing newline"      "$NN" 'comm u1 u2'
run_case "spaces are content"       "$SP" 'comm w1 w2'
run_case "zero terminated"          "$Z1" 'comm -z v1 v2'
run_case "zero total"               "$Z1" 'comm -z --total v1 v2'
run_case "zero no terminator"       'printf "a\0b" > a; printf "b\0c" > b' 'comm -z a b'
run_case "zero long form"           "$Z1" 'comm --zero-terminated v1 v2'
run_case "zero delimiter"           "$Z1" 'comm -z --output-delimiter=, v1 v2'

run_case "unsorted simple"          "$U1" 'comm j1 j2'
run_case "unsorted total"           "$U1" 'comm --total j1 j2'
run_case "unsorted nocheck"         "$U1" 'comm --nocheck-order j1 j2'
run_case "unsorted check"           "$U1" 'comm --check-order j1 j2'
run_case "unsorted check total"     "$U1" 'comm --check-order --total j1 j2'
run_case "unsorted masked"          "$U2" 'comm k1 k2'
run_case "unsorted masked check"    "$U2" 'comm --check-order k1 k2'
run_case "unsorted masked total"    "$U2" 'comm --total k1 k2'
run_case "unsorted first"           "$U3" 'comm l1 l2'
run_case "unsorted reversed"        "$U4" 'comm m1 m2'
run_case "unsorted blank line"      "$U5" 'comm n1 n2'
run_case "unsorted both"            "$U6" 'comm o1 o2'
run_case "unsorted both check"      "$U6" 'comm --check-order o1 o2'
run_case "unsorted late"            "$U7" 'comm p1 p2'
run_case "unsorted disjoint"        "$U8" 'comm q1 q2'
run_case "unsorted disjoint check"  "$U8" 'comm --check-order q1 q2'
run_case "unsorted no columns"      "$U3" 'comm -123 l1 l2'
run_case "check then nocheck"       "$U1" 'comm --check-order --nocheck-order j1 j2'
run_case "nocheck then check"       "$U1" 'comm --nocheck-order --check-order j1 j2'
run_case "check on sorted"          "$S1" 'comm --check-order f1 f2'

run_case "stdin first"              "$S1" 'comm - f2 < f1'
run_case "stdin second"             "$S1" 'comm f1 - < f2'
run_case "stdin both"               "$S1" 'comm - - < f1'
run_case "stdin both total"         "$S1" 'comm --total - - < f1'
run_case "stdin unsorted"           "$U1" 'comm - - < j1'
run_case "zero stdin"               "$Z1" 'comm -z - - < v1'
run_case "double dash"              "$S1" 'comm -- f1 f2'
run_case "dash after double dash"   "$S1" 'comm -- - f2 < f1'
run_case "options after operands"   "$S1" 'comm f1 f2 --total'
run_case "option between operands"  "$S1" 'comm --total f1 -1 f2'

run_case "missing file"             "$S1" 'comm nope f2'
run_case "missing second"           "$S1" 'comm f1 nope'
run_case "missing both"             "$S1" 'comm nope nope2'
run_case "missing name space"       "$S1" 'comm "no pe" f2'
run_case "directory"                "$S1" 'comm /tmp f2'
run_case "no operands"              "$S1" 'comm'
run_case "one operand"              "$S1" 'comm f1'
run_case "one operand quoted"       "$S1" 'comm "a b"'
run_case "three operands"           "$S1" 'comm f1 f2 f1'
run_case "four operands"            "$S1" 'comm f1 f2 f1 f2'
run_case "bad option"               "$S1" 'comm -Q f1 f2'
run_case "bad long option"          "$S1" 'comm --nonsense f1 f2'
run_case "missing delimiter arg"    "$S1" 'comm f1 f2 --output-delimiter'
run_case "bundle"                   "$S1" 'comm -12z f1 f2'
run_case "help"                     "$S1" 'comm --help | head -1'

summary