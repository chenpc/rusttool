#!/usr/bin/env bash
# Differential test for env against GNU env.
#
# The environment this tool prints is part of its output, but both sides run from
# the same shell with the same PATH, so the inherited environment compares equal
# on its own. What does not compare equal is anything order-sensitive, which is
# why there are `-v`, `-S` and signal cases here rather than a single print.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=env
SYS=/usr/bin/env
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

SQ="'"      # a single quote, for the cases that need one inside a case
ESC='\'     # a backslash

# --- printing the environment ---------------------------------------------
run_case "print empty"        '' 'env -i'
run_case "print"              '' 'env'
run_case "print nul"          '' 'env -0'
run_case "print long i"       '' 'env --ignore-environment'
run_case "print long 0"       '' 'env --null'
run_case "print dash"         '' 'env -'
run_case "print dash and i"   '' 'env - -i'
run_case "print set"          '' 'env NEW=1'
run_case "print set empty"    '' 'env NEW='
run_case "print empty name"   '' 'env =1'
run_case "print empty both"   '' 'env ='
run_case "print set twice"    '' 'env A=9 A=10'
run_case "print empty name first" '' 'env -i =x A=1'
run_case "print empty name last"  '' 'env -i A=1 =x'
run_case "print empty name replaced" '' 'env -i =x =y'
run_case "print empty name between" '' 'env -i A=1 =x B=2'
run_case "print empty name then set" '' 'env =x A=1'
run_case "print empty name twice" '' 'env -i ==x'
run_case "print nul with set" '' 'env -0 NEW=1'

# --- unset -----------------------------------------------------------------
run_case "unset one"          '' 'env -u B'
run_case "unset long"         '' 'env --unset=B'
run_case "unset long spaced"  '' 'env --unset B'
run_case "unset attached"     '' 'env -uB'
run_case "unset missing"      '' 'env -u NOPE'
run_case "unset empty name"   '' 'env -u ""'
run_case "unset twice"        '' 'env -u A -u B'
run_case "unset then set"     '' 'env -u A A=5'
run_case "unset and i"        '' 'env -u A -i'
run_case "unset with 0"       '' 'env -0 -u B'
run_case "unset bundle"       '' 'env -0u B'
run_case "unset no argument"  '' 'env -u'
run_case "unset abbreviation" '' 'env --uns B'

# --- running a command -----------------------------------------------------
run_case "echo"               '' 'env /bin/echo hi'
run_case "echo with set"      '' 'env X=1 /bin/echo hi'
run_case "echo args"          '' 'env /bin/echo a b c'
run_case "echo option arg"    '' 'env /bin/echo -n hi'
run_case "exit status"        '' 'env /bin/sh -c "exit 42"'
run_case "true"               '' 'env /bin/true'
run_case "dash then set"      '' 'env - A=1'
run_case "assign then echo"   '' 'env A=x B=y /bin/sh -c "echo \$A\$B"'
run_case "assignment as arg"  '' 'env /bin/echo X=1'
run_case "double dash"        '' 'env -- /bin/echo hi'
run_case "bare name in path"  '' 'env echo hi'
run_case "missing command"    '' 'env nosuchcommand'
run_case "missing with space" '' 'env "no such command"'
run_case "not executable"     '' 'env /etc/hostname'
run_case "directory"          '' 'env /tmp'
run_case "empty command"      '' 'env ""'
run_case "bad option"         '' 'env -Q'
run_case "no permutation"     '' 'env A=1 -i /bin/echo hi'
run_case "operand ends scan"  '' 'env /bin/echo -n hi'
run_case "negative number"    '' 'env -1'
run_case "space option"       '' "env -' ' /bin/true"
run_case "tab option"         '' "env -'\t' /bin/true"
run_case "newline option"     '' "env -'\n' /bin/true"

# --- -C --------------------------------------------------------------------
run_case "chdir attached"     '' 'env -C/ /bin/pwd'
run_case "chdir spaced"       '' 'env -C / /bin/pwd'
run_case "chdir long"         '' 'env --chdir=/ /bin/pwd'
run_case "chdir long spaced"  '' 'env --chdir / /bin/pwd'
run_case "chdir missing"      '' 'env -C'
run_case "chdir no command"   '' 'env -C /'
run_case "chdir bad"          '' 'env -C /nosuchdir /bin/true'
run_case "chdir with set"     '' 'env X=1 -C / /bin/pwd'
run_case "chdir abbreviation" '' 'env --ch / /bin/pwd'

# --- -0 with a command is refused ------------------------------------------
run_case "nul with command"   '' 'env -0 /bin/true'
run_case "nul long command"   '' 'env --null /bin/true'

# --- -S --------------------------------------------------------------------
run_case "S plain"            '' 'env -S "A=1 B=2" /bin/true'
run_case "S assigns"          '' 'env -S "A=1 B=2" /bin/sh -c "echo \$A\$B"'
run_case "S attached"         '' "env -S'A=1 B=2' /bin/true"
run_case "S long"             '' 'env --split-string="A=1 B=2" /bin/true'
run_case "S long spaced"      '' 'env --split-string "A=1 B=2" /bin/true'
run_case "S empty"            '' 'env -S "" /bin/echo hi'
run_case "S only spaces"      '' 'env -S "   " /bin/echo hi'
run_case "S single quoted empty"  '' "env -S \"''\" /bin/echo hi"
run_case "S double quoted empty" '' 'env -S "\"\"" /bin/echo hi'
run_case "S two empty args"   '' "env -S \"'' ''\" /bin/echo hi"
run_case "S quoted word"      '' "env -S \"echo 'a b'\" /bin/echo hi"
run_case "S double quoted"    '' 'env -S "echo \"a b\"" /bin/echo hi'
run_case "S quote in dquote"  '' "env -S \"\\\"'\\\"\" /bin/echo hi"
run_case "S unterminated sq"  '' "env -S \"echo 'a\" /bin/echo hi"
run_case "S unterminated dq"  '' 'env -S "echo \"a" /bin/echo hi'
run_case "S hash at start"    '' 'env -S "A=1 # rest" /bin/sh -c "echo \$A"'
run_case "S hash in word"     '' 'env -S "a#b c" /bin/echo hi'
run_case "S hash only"        '' 'env -S "#" /bin/echo hi'
run_case "S hash in quotes"   '' "env -S \"'#'x\" /bin/echo hi"
run_case "S trailing space"   '' 'env -S "A=1 " /bin/echo hi'
run_case "S newline"          '' 'env -S "A=1
B=2" /bin/sh -c "echo \$A\$B"'
run_case "S expand set"       '' 'env -S "echo \${A}" /bin/echo hi'
run_case "S expand unset"     '' 'env -S "echo \${NOPE}x" /bin/echo hi'
run_case "S expand twice"     '' 'env -S "echo \${A}\${A}" /bin/echo hi'
run_case "S expand digit"     '' 'env -S "echo \${1x}" /bin/echo hi'
run_case "S expand in sq"     '' 'env -S "'\''\${A}'\''" /bin/echo hi'
run_case "S expand empty"     '' 'env -S "echo \${C}" /bin/echo hi'
run_case "S expand at start"  '' 'env -S "\${A} x" /bin/echo hi'
run_case "S escape tab"       '' 'env -S "echo a\tb" /bin/echo hi'
run_case "S escape newline"   '' 'env -S "echo a\nb" /bin/echo hi'
run_case "S escape space"     '' 'env -S "echo a\ b" /bin/echo hi'
run_case "S escape underscore" '' 'env -S "echo a\_b" /bin/echo hi'
run_case "S escape underscore dq" '' 'env -S "echo \"a\_b\"" /bin/echo hi'
run_case "S escape c"         '' 'env -S "echo a\cb" /bin/echo hi'
run_case "S escape c in dq"   '' 'env -S "echo \"a\cb\"" /bin/echo hi'
run_case "S escape quote"     '' "env -S \"echo a\\\\'b\" /bin/echo hi"
run_case "S escape backslash" '' 'env -S "echo a\\\\b" /bin/echo hi'
run_case "S escape hash"      '' 'env -S "echo a\#b" /bin/echo hi'
run_case "S escape dollar"    '' 'env -S "echo a\${A}b" /bin/echo hi'
run_case "S backslash at end" '' 'env -S "echo a\\" /bin/echo hi'
run_case "S bad escape"       '' 'env -S "echo a\qb" /bin/echo hi'
run_case "S escape newline join" '' 'env -S "echo a\
b" /bin/echo hi'
run_case "S shebang options"  '' 'env -S "-i A=1" /bin/echo hi'
run_case "S shebang chdir"    '' 'env -S "-i -C/tmp" /bin/pwd'
run_case "S missing value"    '' 'env -S'
run_case "S abbreviation"     '' 'env --split "A=1" /bin/true'

# --- -v --------------------------------------------------------------------
run_case "debug set"          '' 'env -v A=1 /bin/true'
run_case "debug unset"        '' 'env -v -u A /bin/true'
run_case "debug i"            '' 'env -v -i A=1 /bin/true'
run_case "debug dash"         '' 'env -v - A=1'
run_case "debug chdir"        '' 'env -v -C / /bin/true'
run_case "debug exec"         '' 'env -v /bin/echo hi'
run_case "debug exec space"   '' 'env -v /bin/echo "a b c"'
run_case "debug exec quote"   '' "env -v /bin/echo \"a'b\""
run_case "debug exec tab"     '' "env -v /bin/echo \"a\tb\""
run_case "debug split"        '' 'env -v -S "A=1 B=2" /bin/true'
run_case "debug split one"    '' 'env -v -S "A=1" /bin/true'
run_case "debug expand"       '' 'env -v -S "\${A} \${NOPE}" /bin/true'
run_case "debug signals"      '' 'env -v --block-signal=INT /bin/true'
run_case "debug bundle"       '' 'env -v0 -u A /bin/true'
run_case "debug long"         '' 'env --debug /bin/true'

# --- signal options --------------------------------------------------------
run_case "signal alone"       '' 'env --list-signal-handling /bin/true'
run_case "block int"          '' 'env --list-signal-handling --block-signal=INT /bin/true'
run_case "block two"          '' 'env --list-signal-handling --block-signal=INT,USR1 /bin/true'
run_case "block no value"     '' 'env --list-signal-handling --block-signal /bin/true'
run_case "block empty value"  '' 'env --list-signal-handling --block-signal= /bin/true'
run_case "block comma runs"   '' 'env --list-signal-handling --block-signal=,,INT,, /bin/true'
run_case "block number"       '' 'env --list-signal-handling --block-signal=2 /bin/true'
run_case "block sig prefix"   '' 'env --list-signal-handling --block-signal=SIGINT /bin/true'
run_case "block lower"        '' 'env --list-signal-handling --block-signal=int /bin/true'
run_case "block alias"        '' 'env --list-signal-handling --block-signal=IO /bin/true'
run_case "block rt"           '' 'env --list-signal-handling --block-signal=RTMIN+1 /bin/true'
run_case "block rtmax"        '' 'env --list-signal-handling --block-signal=SIGRTMAX /bin/true'
run_case "block kill"         '' 'env --list-signal-handling --block-signal=KILL /bin/true'
run_case "block bad"          '' 'env --list-signal-handling --block-signal=NOPE /bin/true'
run_case "block reserved"     '' 'env --list-signal-handling --block-signal=32 /bin/true'
run_case "block exit"         '' 'env --list-signal-handling --block-signal=EXIT /bin/true'
run_case "block zero"         '' 'env --list-signal-handling --block-signal=0 /bin/true'
run_case "block second bad"   '' 'env --list-signal-handling --block-signal=INT,NOPE /bin/true'
run_case "block spaced list"  '' 'env --list-signal-handling --block-signal="INT, USR1" /bin/true'
run_case "ignore term"        '' 'env --list-signal-handling --ignore-signal=TERM /bin/true'
run_case "ignore two"         '' 'env --list-signal-handling --ignore-signal=TERM,INT /bin/true'
run_case "ignore no value"    '' 'env --list-signal-handling --ignore-signal /bin/true'
run_case "ignore bad"         '' 'env --list-signal-handling --ignore-signal=NOPE /bin/true'
run_case "default int"        '' 'env --list-signal-handling --default-signal=INT /bin/true'
run_case "default no value"   '' 'env --list-signal-handling --default-signal /bin/true'
run_case "block then ignore"  '' 'env --list-signal-handling --block-signal=INT --ignore-signal=INT /bin/true'
run_case "ignore then default" '' 'env --list-signal-handling --ignore-signal=INT --default-signal=INT /bin/true'
run_case "signal abbreviation" '' 'env --list-signal-handling --block=INT /bin/true'
run_case "signal list first"  '' 'env --list-signal-handling --ignore-signal=INT /bin/true'

# --- long option spellings -------------------------------------------------
run_case "ambiguous i"        '' 'env --i /bin/true'
run_case "ambiguous de"       '' 'env --de /bin/true'
run_case "ambiguous ig"       '' 'env --ig /bin/true'
run_case "unambiguous deb"    '' 'env --deb /bin/true'
run_case "unknown long"       '' 'env --nonsense /bin/true'
run_case "long with equals"   '' 'env --null=yes /bin/true'
run_case "long unset missing" '' 'env --unset'
run_case "long chdir missing" '' 'env --chdir'
run_case "long split missing" '' 'env --split-string'
run_case "long help"          '' 'env --help | head -1'
run_case "long version"       '' 'env --version | head -1'
run_case "short bundle"       '' 'env -0i A=1 /bin/echo hi'
run_case "short bundle cu"    '' 'env -C/ /bin/pwd'
run_case "short bundle iu"    '' 'env -i -u A'

summary