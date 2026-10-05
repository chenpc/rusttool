#!/usr/bin/env python3
"""Fuzz env against GNU env.

Two things make this tool worth fuzzing and are hard to reach by hand:

* the `-S` splitter, whose state machine has a starting separator, a
  check-before-every-append rule and two ways to jump past the unterminated-quote
  check, so the interesting cases are inputs assembled from the exact characters
  that flip those states;
* the argument vector rewrite that a `-S` string performs, because it restarts
  the option scan, so a split argument that looks like an option changes what the
  rest of the line means.

Random words, random option bundles and random `-S` strings are thrown at both
binaries and stdout, stderr and exit status compared byte for byte.

The environment is pinned for both runs, and `_` and `PWD` are dropped from the
comparison: the shell fills those in from the command it ran, so they can only
ever disagree.
"""
import os
import random
import re
import subprocess
import sys

OURS = "/home/chenpc/git/rusttool/target/debug/env"
GNU = "/usr/bin/env"

# Characters that mean something to the -S state machine, plus a few that only
# matter to the shell that produced them.
SPLIT_ALPHABET = [
    b"A", b"B", b"=", b"1", b" ", b"\t", b"\n", b"\r", b"\\", b"'", b'"',
    b"#", b"$", b"{", b"}", b"_", b"c", b"f", b"n", b"r", b"t", b"v",
    b"a", b"-", b"", b"\\ ", b"\\'", b"\\c", b"\\_", b"${", b"${A}", b"${NOPE}",
    b"${A", b"${1}", b"$A", b"'", b'"', b"\\\\", b"\\#", b"\\$", b"\\t",
]

PLAIN_WORDS = [b"A=1", b"B=2", b"-i", b"-v", b"-0", b"-u", b"B", b"--", b"-",
               b"--null", b"--debug", b"--chdir=/", b"--unset=B", b"-S",
               b"echo", b"/bin/echo", b"hi", b"--block-signal=INT",
               b"--list-signal-handling", b"--ignore-signal=TERM", b"=x",
               b"--nonsense", b"-Q", b"--de"]

SIGNAL_WORDS = [b"INT", b"TERM", b"USR1", b"SIGINT", b"int", b"2", b"32",
                b"RTMIN", b"RTMAX", b"RTMIN+1", b"RTMAX-1", b"EXIT", b"0",
                b"NOPE", b"IOT", b"IO", b"CLD", b"UNUSED", b"", b"1e3",
                b"RTMIN +2", b"RTMAX -0", b"999999999999999999999"]

FLAGS = [
    b"-i", b"-0", b"-v", b"--ignore-environment", b"--null", b"--debug",
    b"--list-signal-handling", b"--block-signal", b"--default-signal",
    b"--ignore-signal", b"--unset", b"--chdir", b"--split-string",
    b"--", b"-", b"-i0", b"-0v", b"-i -0", b"--de", b"--i", b"--nonsense",
    b"--null=1", b"--unset=B", b"--chdir=/", b"--split-string=A=1",
]


def make_split(rng):
    count = rng.randrange(0, 12)
    return b"".join(rng.choice(SPLIT_ALPHABET) for _ in range(count))


def make_args(rng):
    args = []
    for _ in range(rng.randrange(0, 5)):
        pick = rng.random()
        if pick < 0.35:
            args.append(rng.choice(FLAGS))
        elif pick < 0.5:
            args += [rng.choice([b"-u", b"--unset", b"-C", b"--chdir",
                                 b"-S", b"--split-string"]),
                     rng.choice(PLAIN_WORDS)]
        elif pick < 0.62:
            args += [rng.choice([b"--block-signal", b"--ignore-signal",
                                 b"--default-signal"]),
                     rng.choice(SIGNAL_WORDS)]
        elif pick < 0.72:
            # A -S string is the interesting shape, so give it real material.
            args += [rng.choice([b"-S", b"--split-string"]), make_split(rng)]
        elif pick < 0.78:
            args += [b"-S" + make_split(rng)]
        else:
            args.append(rng.choice(PLAIN_WORDS))
    # A command to run, now and then, so the exec path is exercised too.
    if rng.random() < 0.35:
        args.append(rng.choice([b"/bin/true", b"/bin/echo", b"echo", b"/bin/false"]))
        if rng.random() < 0.5:
            args.append(b"hi")
    return args


BASE_ENV = {
    "PATH": "/usr/bin:/bin",
    "A": "1",
    "B": "two",
    "C": "",
    "HOME": "/root",
}

NOISE = re.compile(rb"^(_|PWD)=[^\n]*$", re.M)


def run(binary, args):
    done = subprocess.run([binary] + args, capture_output=True, env=BASE_ENV)
    # The program name is the path the shell resolved, which necessarily differs.
    out = done.stdout.replace(binary.encode(), b"env")
    err = done.stderr.replace(binary.encode(), b"env")
    return NOISE.sub(b"", out), NOISE.sub(b"", err), done.returncode


def main():
    rounds = int(sys.argv[1]) if len(sys.argv) > 1 else 3000
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 20261004
    rng = random.Random(seed)
    failures = 0
    skipped = 0
    round_number = -1
    for round_number in range(rounds):
        args = make_args(rng)
        # --help and --version print this project's name and version, which is
        # not the same text as the system's and never will be; the differential
        # cases check the first line of each instead.
        if any(word in (b"--help", b"--version") for word in args):
            skipped += 1
            continue
        mine = run(OURS, args)
        theirs = run(GNU, args)
        if mine != theirs:
            failures += 1
            print("MISMATCH", args)
            for label, got, want in (
                ("stdout", mine[0], theirs[0]),
                ("stderr", mine[1], theirs[1]),
                ("exit", str(mine[2]).encode(), str(theirs[2]).encode()),
            ):
                if got != want:
                    print("  %s ours: %r" % (label, got[:400]))
                    print("  %s gnu : %r" % (label, want[:400]))
            if failures >= 10:
                break
    print("==> %d rounds, %d skipped for --help/--version, %d mismatches"
          % (round_number + 1, skipped, failures))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())