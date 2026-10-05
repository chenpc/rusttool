#!/usr/bin/env python3
"""Fuzz comm against GNU comm.

The hand-written cases can only cover the inputs I thought of; this throws
random line sets, random flag combinations and random operand shapes at both
binaries and compares stdout, stderr and exit status byte for byte.
"""
import os
import random
import subprocess
import sys
import tempfile

OURS = "/home/chenpc/git/rusttool/target/debug/comm"
GNU = "/usr/bin/comm"

ALPHABET = [b"a", b"b", b"c", b"d", b"", b"a b", b"a-b", b"A", b"z z", b"aa"]

FLAG_POOL = [
    ["-1"], ["-2"], ["-3"], ["-12"], ["-13"], ["-23"], ["-123"],
    ["--total"], ["--check-order"], ["--nocheck-order"],
    ["--output-delimiter=,"], ["--output-delimiter="], ["--output-delimiter=XY"],
    ["--output-delimiter=|"], ["-z"], ["--zero-terminated"],
    ["-1", "--total"], ["-3", "--output-delimiter=."], ["-2", "-z"],
]


def make_lines(rng, terminator):
    count = rng.randrange(0, 6)
    body = terminator.join(rng.choice(ALPHABET) for _ in range(count))
    if count and rng.random() < 0.8:
        body += terminator
    if rng.random() < 0.1:
        body = b""
    return body


def run(binary, args, workdir, stdin_bytes):
    done = subprocess.run(
        [binary] + args, capture_output=True, cwd=workdir, input=stdin_bytes
    )
    err = done.stderr.replace(binary.encode(), b"comm")
    return done.stdout, err, done.returncode


def main():
    rounds = int(sys.argv[1]) if len(sys.argv) > 1 else 400
    rng = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 20261004)
    work = tempfile.mkdtemp()
    bad = 0
    for round_number in range(rounds):
        terminator = b"\0" if rng.random() < 0.2 else b"\n"
        for name in ("f1", "f2"):
            with open(os.path.join(work, name), "wb") as handle:
                handle.write(make_lines(rng, terminator))
        flags = []
        for candidate in FLAG_POOL:
            if rng.random() < 0.18:
                flags.extend(candidate)
        shape = rng.random()
        stdin_bytes = b""
        if shape < 0.15:
            args = flags + ["-", "-"]
            stdin_bytes = make_lines(rng, terminator)
        elif shape < 0.25:
            args = flags + ["-", "f2"]
            stdin_bytes = make_lines(rng, terminator)
        elif shape < 0.35:
            args = flags + ["f1", "-"]
            stdin_bytes = make_lines(rng, terminator)
        elif shape < 0.45:
            args = flags
        else:
            args = flags + ["f1", "f2"]

        ours = run(OURS, args, work, stdin_bytes)
        theirs = run(GNU, args, work, stdin_bytes)
        if ours != theirs:
            bad += 1
            print("MISMATCH round %d args=%r stdin=%r" % (round_number, args, stdin_bytes))
            for name in ("f1", "f2"):
                with open(os.path.join(work, name), "rb") as handle:
                    print("  %s = %r" % (name, handle.read()))
            print("  ours  = %r" % (ours,))
            print("  gnu   = %r" % (theirs,))
            if bad >= 5:
                break
    os.system("rm -rf " + work)
    print("fuzzed %d rounds, %d mismatches" % (rounds, bad))
    return 1 if bad else 0


sys.exit(main())