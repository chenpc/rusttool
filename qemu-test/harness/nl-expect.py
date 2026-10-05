#!/usr/bin/env python3
"""Generate the nl guest case: every expected value comes from /usr/bin/nl."""

import subprocess
import textwrap

W = "/tmp/opencode/nl-case"
subprocess.run(["rm", "-rf", W], check=True)
subprocess.run(["mkdir", "-p", W], check=True)

FIXTURES = {
    "pages": ["H1", "\\:", "", "B1", "B2", "\\:\\:", "", "B3", "\\:\\:\\:", "F1", "", "F2"],
    "tabs": ["a", "", "b", "", "", "c"],
    "words": ["x y", "p q", "r s"],
}
for name, lines in FIXTURES.items():
    with open("%s/%s" % (W, name), "w") as handle:
        handle.write("\n".join(lines) + "\n")


def gnu(args):
    done = subprocess.run(["/usr/bin/nl"] + args, capture_output=True)
    return done.stdout.decode(), done.stderr.decode(), done.returncode


def show(args):
    out, err, code = gnu(args)
    print("=== nl %s" % " ".join(args))
    print("exit=%d" % code)
    print("out=%r" % out)
    if err:
        print("err=%r" % err)
    print()


CASES = [
    ("nl-basic", ["-ba", W + "/tabs"], None),
    ("nl-default", [W + "/pages"], None),
    ("nl-default-blank", [W + "/tabs"], None),
    ("nl-no-number", ["-bn", W + "/pages"], None),
    ("nl-header-footer", ["-hha", "-ffa", W + "/pages"], None),
    ("nl-header-only", ["-hta", W + "/pages"], None),
    ("nl-footer-t", ["-ft", W + "/pages"], None),
    ("nl-no-renumber", ["-ba", "-p", W + "/pages"], None),
    ("nl-start-0", ["-ba", "-v0", W + "/pages"], None),
    ("nl-start-negative", ["-ba", "-v-3", W + "/tabs"], None),
    ("nl-increment-2", ["-ba", "-i2", W + "/tabs"], None),
    ("nl-increment-negative", ["-ba", "-i-1", W + "/tabs"], None),
    ("nl-width-3", ["-ba", "-w3", W + "/tabs"], None),
    ("nl-width-1", ["-ba", "-w1", W + "/tabs"], None),
    ("nl-format-left", ["-ba", "-nln", "-w4", W + "/tabs"], None),
    ("nl-format-zero", ["-ba", "-nrz", "-w4", W + "/tabs"], None),
    ("nl-format-zero-negative", ["-ba", "-nrz", "-v-2", "-w5", W + "/tabs"], None),
    ("nl-separator-dots", ["-ba", "-s.", "-w3", W + "/tabs"], None),
    ("nl-separator-empty", ["-ba", "-s", "-w3", W + "/tabs"], None),
    ("nl-separator-many", ["-ba", "-s' | '", "-w3", W + "/tabs"], None),
    ("nl-join-2", ["-ba", "-l2", W + "/tabs"], None),
    ("nl-join-3", ["-ba", "-l3", W + "/tabs"], None),
    ("nl-join-style-t", ["-bt", "-l2", W + "/tabs"], None),
    ("nl-pattern", ["-bp'[0-9]'", W + "/words"], None),
    ("nl-pattern-none", ["-bpzzz", W + "/words"], None),
    ("nl-pattern-anchored", ["-bp'^p'", W + "/words"], None),
    ("nl-pattern-bracket", ["-bp'[xy]'", W + "/words"], None),
    ("nl-pattern-group", ["-bp'\\(x\\|y\\) '", W + "/words"], None),
    ("nl-pattern-header", ["-bpn", "-hp'^H'", W + "/pages"], None),
    ("nl-pattern-footer", ["-bpn", "-fp'^F'", W + "/pages"], None),
]

for name, args, _ in CASES:
    show(args)

print("### rendered")
print()
for name, args, _ in CASES:
    out, _, _ = gnu(args)
    print("--- %s: %s" % (name, " ".join(args)))
    print(repr(out))