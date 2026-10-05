#!/usr/bin/env python3
"""Fuzz unexpand against GNU unexpand.

Same shape as the expand fuzzer: the tab-stop grammar, `-a` versus `--first-only`,
backspaces and files without a trailing newline all interact, and the
interactions are where a hand-written case list stops reaching.
"""
import os
import random
import subprocess
import sys
import tempfile

OURS = "/home/chenpc/git/rusttool/target/debug/unexpand"
GNU = "/usr/bin/unexpand"

BYTES = [b"a", b"b", b" ", b"\t", b"\n", b"\x08", b"z", b"Z", b"0"]


def make_input(rng):
    length = rng.randrange(0, 22)
    body = b"".join(rng.choice(BYTES) for _ in range(length))
    if rng.random() < 0.7:
        body += b"\n"
    return body


def make_stops(rng):
    if rng.random() < 0.2:
        return ""
    count = rng.randrange(1, 4)
    parts = []
    for _ in range(count):
        prefix = rng.choice(["", "", "", "/", "+"])
        digits = "".join(rng.choice("0123456789") for _ in range(rng.randrange(1, 4)))
        parts.append(prefix + digits)
    text = rng.choice([",", ",", ",", " "]).join(parts)
    if rng.random() < 0.1:
        text += rng.choice(["x", "/", "+", "99999999999999999999", "-", ","])
    return text


def run(binary, args, workdir, stdin_bytes):
    done = subprocess.run(
        [binary] + args, capture_output=True, cwd=workdir, input=stdin_bytes
    )
    err = done.stderr.replace(binary.encode(), b"unexpand")
    return done.stdout, err, done.returncode


def main():
    rounds = int(sys.argv[1]) if len(sys.argv) > 1 else 400
    rng = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 20261004)
    work = tempfile.mkdtemp()
    bad = 0
    for round_number in range(rounds):
        payloads = {"f1": make_input(rng)}
        if rng.random() < 0.5:
            payloads["f2"] = make_input(rng)
        for name, body in payloads.items():
            with open(os.path.join(work, name), "wb") as handle:
                handle.write(body)

        args = []
        for _ in range(rng.randrange(0, 3)):
            args.append("-t")
            args.append(make_stops(rng))
        if rng.random() < 0.3:
            args.append("-a")
        if rng.random() < 0.15:
            args.append("--first-only")
        if rng.random() < 0.15:
            args.append(rng.choice("123456789"))
        if rng.random() < 0.1:
            args.append(",")
        if rng.random() < 0.15:
            args.append("-")
            stdin_bytes = make_input(rng)
        else:
            args.extend(sorted(payloads))
            stdin_bytes = b""

        ours = run(OURS, args, work, stdin_bytes)
        theirs = run(GNU, args, work, stdin_bytes)
        if ours != theirs:
            bad += 1
            print("MISMATCH round %d args=%r" % (round_number, args))
            print("  files = %r" % payloads)
            print("  stdin = %r" % stdin_bytes)
            print("  ours  = %r" % (ours,))
            print("  gnu   = %r" % (theirs,))
            if bad >= 5:
                break
    os.system("rm -rf " + work)
    print("fuzzed %d rounds, %d mismatches" % (rounds, bad))
    return 1 if bad else 0


sys.exit(main())