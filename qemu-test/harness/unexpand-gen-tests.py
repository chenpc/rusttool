#!/usr/bin/env python3
"""Replace unexpand's behavioural tests with expectations taken from GNU.

Hand-written expectations for a tool with this much bookkeeping are a reliable
way to test the wrong thing, so this writes what `/usr/bin/unexpand` actually
prints.
"""
import subprocess

NL = chr(10)
BS = chr(8)
TAB = chr(9)

# (input, extra flags)
CASES = [
    ("        a", []),
    ("  b", []),
    ("  " + TAB + "c", []),
    (TAB + "c", []),
    ("a  " + NL + "b  " + NL, []),
    ("        a" + NL + "        b" + NL, []),
    ("    a", []),
    ("a        b", ["-a"]),
    ("a " + NL, ["-a"]),
    ("a  " + NL, ["-a"]),
    ("a        b" + NL + "a " + NL, ["-a"]),
    ("a " + NL + "  b" + NL, ["-a"]),
    ("a ", ["-a"]),
    ("a  ", ["-a"]),
    ("a  ", []),
    ("        a", []),
    ("        a", ["-a"]),
    ("a       b", ["-a"]),
    ("a   b", ["-a"]),
    ("a    b", ["-a"]),
    ("  a  b", ["-a"]),
    (" " + TAB + "a", ["-a"]),
    (TAB + " a", ["-a"]),
    ("a" + TAB + " b", ["-a"]),
    ("a       " + BS + "b", ["-a"]),
    ("a       " + BS + BS + "b", ["-a"]),
    (BS + "  a", ["-a"]),
    ("        a", ["-t", "4"]),
    ("  b", ["-t", "2"]),
    ("        a", ["-t", "2"]),
    ("        a", ["-t", "2,6"]),
    ("a  b", ["-a", "-t", "2"]),
    ("a   b", ["-a", "-t", "2"]),
    ("a    b", ["-a", "-t", "2"]),
    ("a        b", ["-a", "-t", "2"]),
    ("a b", ["-a", "-t", "2"]),
    (" b", ["-a", "-t", "2"]),
    ("  a        b", ["-a", "-t", "2"]),
]


def rust(text):
    out = text.replace("\\", "\\\\").replace('"', '\\"')
    out = out.replace(TAB, "\\t").replace(BS, "\\x08").replace(NL, "\\n")
    return '"%s"' % out


def gnu(flags, body):
    done = subprocess.run(
        ["/usr/bin/unexpand"] + flags, input=body.encode(), capture_output=True
    )
    return done.stdout.decode("utf-8", "replace")


HEADER = '''    #[test]
    fn every_case_agrees_with_gnu() {
        // One test for the whole grid, so a new case here cannot be split off
        // from the expectations that GNU produced for it.
        let eight = TabStops::default();
        let four = stops_of("4");
        let two = stops_of("2");
        let two_six = stops_of("2,6");
        let cases: &[(&str, &TabStops, Scope, &str)] = &[
'''

FOOTER = '''        ];
        for (input, stops, scope, expected) in cases {
            assert_eq!(
                &run(input, stops, *scope),
                *expected,
                "{:?} {:?}",
                input,
                scope
            );
        }
    }
'''

lines = [HEADER]
for body, flags in CASES:
    stops = "eight"
    if "-t" in flags:
        stops = {"4": "four", "2": "two", "2,6": "two_six"}[flags[-1]]
    scope = "Scope::All" if "-a" in flags else "Scope::FirstOnly"
    out = gnu(flags, body)
    lines.append(
        "            (%s, &%s, %s, %s),\n" % (rust(body), stops, scope, rust(out))
    )
lines.append(FOOTER)

path = "tools/unexpand/src/lib.rs"
source = open(path).read()
END = "    #[test]\n    fn stop_errors_match_coreutils()"
# The block to replace is either the hand-written tests or a previous run of
# this generator, so the script can be re-run after adding a case.
STARTS = [
    "    #[test]\n    fn the_default_converts_only_the_leading_blanks()",
    "    #[test]\n    fn every_case_agrees_with_gnu()",
]
for marker in STARTS:
    if marker in source:
        start = source.index(marker)
        break
else:
    raise SystemExit("no start marker found")
end = source.index(END)
open(path, "w").write(source[:start] + "".join(lines) + source[end:])
print("rewrote the behavioural tests")
