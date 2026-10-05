#!/usr/bin/env python3
"""Generate the unexpand guest case: every expected value comes from GNU."""
import subprocess

W = "/tmp/opencode/unexpand-case"

FILES = {
    "lead": b"        a\n  b\n  \tc\n",
    "mid": b"a        b\na \na       b\na   b\n",
    "blanks": b"\t \t\ta\n \t \t b\n",
    "bs": b"a       \bd\nb       \bb\n",
    "nonl": b"    a",
    "two": b"q        r\n",
}

CASES = [
    ("default", ["lead"], None),
    ("default-mid", ["mid"], None),
    ("default-blanks", ["blanks"], None),
    ("default-bs", ["bs"], None),
    ("default-nonl", ["nonl"], None),
    ("two-files", ["lead", "two"], None),
    ("all", ["mid"], "-a"),
    ("all-long", ["mid"], "--all"),
    ("all-blanks", ["blanks"], "-a"),
    ("all-bs", ["bs"], "-a"),
    ("all-nonl", ["nonl"], "-a"),
    ("first-only", ["mid"], "--first-only"),
    ("all-then-first-only", ["mid"], "-a --first-only"),
    ("t2", ["lead"], "-t 2"),
    ("t2-all", ["mid"], "-a -t 2"),
    ("t4", ["lead"], "-t 4"),
    ("t4-long", ["lead"], "--tabs=4"),
    ("list", ["lead"], "-t 2,6"),
    ("list-all", ["mid"], "-a -t 2,6"),
    ("digit", ["lead"], "-2"),
    ("t8-all", ["mid"], "-t 8"),
    ("extend", ["mid"], "-a -t 2,4,/3"),
    ("increment", ["mid"], "-a -t 2,+3"),
    ("stdin", ["lead"], None),
]

ERRORS = [
    ("zero", ["-t", "0", "lead"]),
    ("descending", ["-t", "4,2", "lead"]),
    ("bad-character", ["-t", "4x,2", "lead"]),
    ("misplaced-specifier", ["-t", "4+x", "lead"]),
    ("too-large", ["-t", "99999999999999999999", "lead"]),
    ("repeated-slash", ["-t", "/3", "-t", "/4", "lead"]),
    ("slash-and-plus", ["-t", "/3", "-t", "+1", "lead"]),
    ("missing-file", ["nope"]),
    ("missing-arg", ["lead", "-t"]),
    ("bad-option", ["-Q", "lead"]),
]

import os
import tempfile

work = tempfile.mkdtemp()
for name, body in FILES.items():
    with open(os.path.join(work, name), "wb") as handle:
        handle.write(body)


def bash_single(text):
    return "'" + text.replace("'", "'\\''") + "'"


def show(label, flags, names, stdin):
    args = ["/usr/bin/unexpand"] + (flags.split() if flags else [])
    args += [os.path.join(work, name) for name in names]
    done = subprocess.run(args, capture_output=True, cwd=work, input=stdin)
    err = done.stderr.replace(b"/usr/bin/unexpand", b"unexpand")
    print("--- %s :: %s %s" % (label, flags or "", " ".join(names)))
    print("    rc  = %d" % done.returncode)
    print("    out = %r" % done.stdout)
    if err:
        print("    err = %r" % err)


for label, names, flags in CASES:
    if label == "stdin":
        stdin = open(os.path.join(work, "lead"), "rb").read()
        show(label, "", [], stdin)
        continue
    show(label, flags, names, b"")

for label, args in ERRORS:
    full = ["/usr/bin/unexpand"] + args
    done = subprocess.run(full, capture_output=True, cwd=work, input=b"")
    err = done.stderr.replace(b"/usr/bin/unexpand", b"unexpand")
    err = err.replace(work.encode(), b"$W")
    print("--- %s :: %s" % (label, " ".join(args)))
    print("    rc  = %d" % done.returncode)
    print("    out = %r" % done.stdout)
    if err:
        print("    err = %r" % err)

os.system("rm -rf " + work)
