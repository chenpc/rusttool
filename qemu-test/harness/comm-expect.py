#!/usr/bin/env python3
"""Generate comm's unit-test expectations straight from GNU comm."""
import subprocess
import sys

CASES = [
    ("default", ["f1", "f2"], None),
    ("identical", ["s", "s"], None),
    ("no first", ["f1", "f2"], "-1"),
    ("no second", ["f1", "f2"], "-2"),
    ("no both", ["f1", "f2"], "-3"),
    ("no 1 and 2", ["f1", "f2"], "-12"),
    ("no all", ["f1", "f2"], "-123"),
    ("delim", ["f1", "f2"], "--output-delimiter= | "),
    ("empty delim", ["f1", "f2"], "--output-delimiter="),
    ("total", ["f1", "f2"], "--total"),
    ("total no 1", ["f1", "f2"], "--total -1"),
    ("empty right", ["f1", "empty"], None),
    ("empty left", ["empty", "f1"], None),
    ("empty both", ["empty", "empty"], None),
    ("blank lines", ["b1", "b2"], None),
    ("blank lines nocheck", ["b1", "b2"], "--nocheck-order"),
    ("prefix", ["p1", "p2"], None),
    ("dupes", ["d1", "d2"], None),
]

# The files the cases refer to, written into a scratch directory.
FILES = {
    "f1": b"a\nb\nc\n",
    "f2": b"b\nc\nd\n",
    "s": b"a\nb\nc\n",
    "empty": b"",
    "b1": b"a\n\nb\n",
    "b2": b"\na\n",
    "p1": b"a b\nb\n",
    "p2": b"a\n",
    "d1": b"a\na\nb\n",
    "d2": b"a\n",
}

import os
import tempfile

work = tempfile.mkdtemp()
for name, body in FILES.items():
    with open(os.path.join(work, name), "wb") as handle:
        handle.write(body)


def gnu(flags, names):
    args = ["/usr/bin/comm"]
    if flags:
        args.extend(flags.split())
    args.extend(os.path.join(work, name) for name in names)
    done = subprocess.run(args, capture_output=True)
    err = done.stderr.replace(work.encode(), b"")
    return done.stdout, err, done.returncode


for label, names, flags in CASES:
    out, err, code = gnu(flags, names)
    print("--- %s" % label)
    print("    out = %r" % out)
    if err:
        print("    err = %r" % err)
    print("    rc  = %d" % code)

print()
print("=== disordered ===")
for left, right in [("b\na\n", "a\nb\nc\n"), ("c\nb\n", "c\n"), ("d\nc\nb\na\n", "a\n")]:
    with open(os.path.join(work, "x"), "wb") as handle:
        handle.write(left.encode())
    with open(os.path.join(work, "y"), "wb") as handle:
        handle.write(right.encode())
    for flags in [None, "--nocheck-order", "--check-order", "--total"]:
        out, err, code = gnu(flags, ["x", "y"])
        print("--- %r vs %r %s" % (left, right, flags or ""))
        print("    out = %r" % out)
        print("    err = %r" % err)
        print("    rc  = %d" % code)

os.system("rm -rf " + work)
sys.exit(0)