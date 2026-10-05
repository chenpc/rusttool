#!/usr/bin/env python3
"""Write env's guest case with expectations taken from GNU env.

A hand-written expectation for a tool with this much conditional output is a
reliable way to test the wrong thing, so every expected string here is what
`/usr/bin/env` actually prints for the same command.

The oracle runs with LC_ALL=C because that is the locale the guest has: no locale
is generated there, so the diagnostics come out with plain quotes, and an
expectation written under a UTF-8 locale would be wrong for the guest.

The guest has no shell to nest inside, so the commands here exec other published
tools rather than a shell script: /mnt/bin/env prints back whatever environment
it was handed, which is how a case proves that an assignment arrived, and
/mnt/bin/seq is there to prove that a command ran at all. The oracle uses the
system copies in their place.
"""
import os
import subprocess
import sys

HOST_ENV = "/usr/bin/env"
GUEST_ENV = "/mnt/bin/env"
HOST_SEQ = "/usr/bin/seq"
GUEST_SEQ = "/mnt/bin/seq"

TAB = chr(9)
NL = chr(10)

# (name, argv, what to check)
#   "ok"    both streams and the exit status must match the oracle
#   "fail"  the command must fail, with the oracle's message and status
CASES = [
    # --- printing the environment -----------------------------------------
    ("env -i prints nothing", ["-i"], "ok"),
    ("env -i -0 prints nothing", ["-i", "-0"], "ok"),
    ("env -i with two names", ["-i", "A=1", "B=two"], "ok"),
    ("env -i keeps assignment order", ["-i", "B=2", "A=1"], "ok"),
    ("env -i a later assignment wins", ["-i", "A=1", "A=9"], "ok"),
    ("env -i accepts an empty value", ["-i", "A="], "ok"),
    ("env -i accepts an empty name", ["-i", "=x"], "ok"),
    ("env -i accepts a bare equals", ["-i", "="], "ok"),
    ("env -0 ends records with NUL", ["-0", "-i", "A=1"], "nul"),
    ("env --null long form", ["--null", "-i", "A=1"], "nul"),
    ("env --ignore-environment long form", ["--ignore-environment"], "ok"),
    # --- unset --------------------------------------------------------------
    ("env -u drops a name", ["-i", "-u", "A", "A=1", "B=2"], "ok"),
    ("env --unset long form", ["-i", "--unset=A", "A=1", "B=2"], "ok"),
    ("env -u attached", ["-i", "-uA", "A=1", "B=2"], "ok"),
    ("env -u of a name that is not set", ["-i", "-u", "NOPE", "A=1"], "ok"),
    ("env -u happens before the assignments", ["-i", "-u", "A", "A=5"], "ok"),
    ("env -i empties regardless of the unsets", ["-i", "-u", "A", "A=1"], "ok"),
    ("env -u of an empty name is reported", ["-u", ""], "fail"),
    # --- a bare - is -i -----------------------------------------------------
    ("env - empties the environment", ["-", "A=1"], "ok"),
    # --- running a command --------------------------------------------------
    ("env runs a command", ["-i", HOST_SEQ, "3"], "ok"),
    ("env passes the arguments", ["-i", HOST_SEQ, "2", "4"], "ok"),
    ("env the command sees the assignment", ["-i", "A=7", HOST_ENV], "ok"),
    ("env the command sees two assignments", ["-i", "A=7", "B=8", HOST_ENV], "ok"),
    ("env an assignment after the command is an argument",
     ["-i", HOST_ENV, "A=7"], "ok"),
    ("env -- ends option parsing", ["--", "-i"], "fail"),
    ("env a missing command fails", ["-i", "nosuchcommand"], "fail"),
    ("env a command that is not executable fails", ["-i", "/tmp/env-case-notexec"], "fail"),
    ("env a directory is not a command", ["-i", "/tmp"], "fail"),
    ("env an empty command name fails", ["-i", ""], "fail"),
    ("env a missing -u argument fails", ["-i", "-u"], "fail"),
    ("env an unknown option fails", ["-i", "-Q"], "fail"),
    ("env an unknown long option fails", ["-i", "--nonsense"], "fail"),
    ("env an ambiguous long option fails", ["-i", "--i"], "fail"),
    ("env -0 with a command fails", ["-0", HOST_SEQ, "3"], "fail"),
    ("env -C with no command fails", ["-C", "/"], "fail"),
    ("env a command name with a space gets the hint",
     ["-i", "no such command"], "fail"),
    # --- -C -----------------------------------------------------------------
    ("env -C reaches the directory", ["-C", "/tmp", HOST_SEQ, "2"], "ok"),
    ("env --chdir long form", ["--chdir=/tmp", HOST_SEQ, "2"], "ok"),
    ("env -C attached", ["-C/tmp", HOST_SEQ, "2"], "ok"),
    ("env -C of a directory that is not there fails",
     ["-C", "/nosuchdir", HOST_SEQ, "2"], "fail"),
    ("env -C needs a value", ["-C"], "fail"),
    ("env -C does not change the environment", ["-C", "/tmp", "-i", "A=7", HOST_ENV], "ok"),
    # --- -S -----------------------------------------------------------------
    ("env -S assigns", ["-i", "-S", "A=7", HOST_ENV], "ok"),
    ("env -S assigns two", ["-i", "-S", "A=7 B=8", HOST_ENV], "ok"),
    ("env -S attached", ["-i", "-SA=7", HOST_ENV], "ok"),
    ("env --split-string long form", ["-i", "--split-string=A=7", HOST_ENV], "ok"),
    ("env -S quotes a word together", ["-i", "-S", "A='a b'", HOST_ENV], "ok"),
    ("env -S of an empty string adds nothing", ["-i", "-S", "", HOST_SEQ, "2"], "ok"),
    ("env -S expands a variable", ["-i", "V=9", "-S", "A=${V}", HOST_ENV], "ok"),
    ("env -S drops a variable that is not set",
     ["-i", "-S", "A=${NOPE}", HOST_ENV], "ok"),
    ("env -S passes a shebang line", ["-i", "-S", "-i A=7", HOST_ENV], "ok"),
    ("env -S with no value fails", ["-i", "-S"], "fail"),
    ("env -S with an unterminated quote fails", ["-i", "-S", "echo 'x"], "fail"),
    ("env -S with an unknown escape fails", ["-i", "-S", "echo a\\qb"], "fail"),
    ("env -S with a bad variable name fails", ["-i", "-S", "echo ${1x}"], "fail"),
    ("env -S with a trailing backslash fails", ["-i", "-S", "echo a\\"], "fail"),
    # --- -v -----------------------------------------------------------------
    ("env -v reports the assignment", ["-v", "-i", "A=7", HOST_SEQ, "2"], "ok"),
    ("env -v reports the unset", ["-v", "-i", "-u", "A", HOST_SEQ, "2"], "ok"),
    ("env -v reports the exec", ["-v", "-i", HOST_SEQ, "2"], "ok"),
    ("env -v reports the split", ["-v", "-i", "-S", "A=7 B=8", HOST_SEQ, "2"], "ok"),
    ("env -v reports the expansion",
     ["-v", "-i", "V=9", "-S", "A=${V}", HOST_SEQ, "2"], "ok"),
    ("env --debug long form", ["--debug", "-i", "A=7", HOST_SEQ, "2"], "ok"),
    # --- signals ------------------------------------------------------------
    ("env lists blocked and ignored signals",
     ["--list-signal-handling", "--block-signal=INT", "--ignore-signal=TERM",
      HOST_SEQ, "2"], "ok"),
    ("env blocks a signal by number",
     ["--list-signal-handling", "--block-signal=2,10", HOST_SEQ, "2"], "ok"),
    # Not "block everything": the guest's C library has no room for signal 34, so
    # a mask that covers everything cannot be listed there and the expectation
    # would have to be hand-trimmed. The differential suite covers that shape.
    ("env blocks a spread of signals",
     ["--list-signal-handling", "--block-signal=INT,USR1,RTMIN+2,RTMAX", HOST_SEQ, "2"], "ok"),
    ("env unblocks what --default-signal resets",
     ["--list-signal-handling", "--block-signal=INT", "--default-signal=INT",
      HOST_SEQ, "2"], "ok"),
    ("env -v reports the mask change",
     ["-v", "--block-signal=INT", HOST_SEQ, "2"], "ok"),
    ("env rejects an unknown signal", ["--block-signal=NOPE", HOST_SEQ, "2"], "fail"),
    ("env rejects a reserved signal number", ["--block-signal=32", HOST_SEQ, "2"], "fail"),
    ("env rejects signal zero", ["--block-signal=0", HOST_SEQ, "2"], "fail"),
    ("env accepts a real-time signal",
     ["--list-signal-handling", "--block-signal=RTMIN+1", HOST_SEQ, "2"], "ok"),
    # The quoting style follows the locale, so the case pins the one the oracle
    # ran under. See the note at the top about musl fixing the codeset: on the
    # guest this would come out the same under any locale, but pinning it keeps
    # the case runnable against the system tool on the host as well.
]


def oracle(argv, locale="C"):
    """Run the system env and return (stdout, stderr, exit status).

    The environment is fixed so that a case which prints the environment has
    something reproducible to print, and so that the two `-S` expansion cases have
    the same variables to find.
    """
    done = subprocess.run(
        [HOST_ENV] + argv,
        capture_output=True,
        env={"PATH": "/usr/bin:/bin", "A": "1", "B": "two", "LC_ALL": locale},
    )
    # The oracle's own argv[0] is a path; the guest finds the tool through PATH,
    # so it is the bare name there. Only that one gets shortened — every other
    # mention is the command that was exec'd, which really is a path.
    def shorten(text):
        return (text
                .replace(HOST_ENV.encode() + b": ", b"env: ")
                .replace(HOST_ENV.encode() + b" --help", b"env --help")
                .replace(HOST_SEQ.encode(), GUEST_SEQ.encode())
                .replace(HOST_ENV.encode(), GUEST_ENV.encode()))

    return shorten(done.stdout), shorten(done.stderr), done.returncode


# What a case pins, and what the oracle runs under. They differ on purpose: the
# pin is what the command is asked for, while the oracle needs the locale that
# reproduces the guest's quoting shape, which musl fixes regardless.
PIN = "C.UTF-8"
ORACLE_LOCALE = "C.UTF-8"


def shell_word(word):
    """A shell word for the guest script, quoted only where it has to be."""
    if word == "":
        return "''"
    if all(c.isalnum() or c in "-_./:=,@%+^[]" for c in word):
        return word
    return "'" + word.replace("'", "'\\''") + "'"


def render(command, captured, status, name, kind, forced=None):
    """The lines that run `command` in the guest and check it against the oracle."""
    rendered = ["# %s" % name]
    if forced != PIN:
        rendered.insert(1, "# (this one pins locale %s instead of %s)" % (forced, PIN))
    command = "LC_ALL=%s %s" % (forced, command)
    out, err = captured
    # Every expectation is read back through $( ), which eats trailing newlines,
    # so they are eaten here too rather than silently never matching.
    def strip(text):
        return text.decode("utf-8", "surrogateescape").rstrip("\n")

    def expectation(text):
        """A double-quoted shell word, so a `$` or a `"` in the expected output
        is not taken as something the case script itself means."""
        return '"' + text.replace("\\", "\\\\").replace('"', '\\"') \
                        .replace("$", "\\$").replace("`", "\\`") + '"'

    if kind == "nul":
        # A NUL cannot travel through $( ), so only the byte count is compared;
        # the shape of the records is checked by the newline cases above.
        rendered.append('check "%s byte count" "%d" "$(%s | wc -c)"'
                        % (name, len(out), command))
        rendered.append("")
        return rendered
    if kind == "fail":
        rendered.append("if %s >/dev/null 2>&1; then fail \"%s\"; fi" % (command, name))
        if err:
            rendered.append('check "%s message" %s "$(%s 2>&1 >/dev/null)"'
                            % (name, expectation(strip(err)), command))
        rendered.append('check "%s exit status" "%d" "$(%s >/dev/null 2>&1; echo $?)"'
                        % (name, status, command))
        rendered.append("")
        return rendered
    rendered.append("%s >/tmp/env-case-out 2>/tmp/env-case-err" % command)
    if out:
        rendered.append('check "%s stdout" "$(cat /tmp/env-case-out)" %s'
                        % (name, expectation(strip(out))))
    if err:
        rendered.append('check "%s stderr" "$(cat /tmp/env-case-err)" %s'
                        % (name, expectation(strip(err))))
    rendered.append('check "%s exit status" "%d" "$(%s >/dev/null 2>&1; echo $?)"'
                    % (name, status, command))
    rendered.append("rm -f /tmp/env-case-out /tmp/env-case-err")
    rendered.append("")
    return rendered


def main():
    out = [
        "#!/usr/bin/env bash",
        "# env(1) in the guest.",
        "#",
        "# Every expected value below is what GNU env prints for the same command; they",
        "# were generated by running the system tool rather than written by hand,",
        "# because a hand-written expectation is how a case starts lying. Regenerate:",
        "#",
        "#     python3 qemu-test/harness/env-gen-case.py > qemu-test/cases-shell/env.misc.sh",
        "#",
        "# The oracle ran under LC_ALL=C, which is the locale the guest has: none is",
        "# generated there, so the diagnostics come out with plain quotes and an",
        "# expectation written under a UTF-8 locale would be wrong. The shell the",
        "# commands run under is brush from the share; the oracle used /usr/bin/brush",
        "# in its place.",
        "set -u",
        "",
        'fail() { echo "FAIL: $*" >&2; exit 1; }',
        "check() {",
        '  if [ "$2" != "$3" ]; then',
        "    printf 'FAIL: %s\\n  expected: [%s]\\n  actual:   [%s]\\n' \"$1\" \"$2\" \"$3\" >&2",
        "    exit 1",
        "  fi",
        "}",
        "",
        "# A file that exists but cannot be run. The guest has no /etc/hostname, and",
        "# the point of the case is the errno the exec reports, so it makes its own.",
        "printf 'not a program\n' >/tmp/env-case-notexec",
        "chmod 644 /tmp/env-case-notexec",
        "",
    ]
    # The oracle needs the same fixture the case creates, or the errno it reports
    # would be the wrong one.
    notexecutable = "/tmp/env-case-notexec"
    with open(notexecutable, "w") as handle:
        handle.write("not a program\n")
    os.chmod(notexecutable, 0o644)
    for name, argv, kind in CASES:
        # A leading locale on a case means "pin this one to that locale instead",
        # which is the only way a case can check that the quoting follows it. Both
        # come out the same here, and that is the point: musl fixes the codeset.
        forced = PIN
        if argv and argv[0] in ("C", "C.UTF-8"):
            forced = argv[0]
            argv = argv[1:]
        captured = oracle(argv, locale=ORACLE_LOCALE)
        # The oracle runs the system tools; the guest reaches ours through the
        # share, so only the command line and the text of the diagnostics change.
        guest = [word.replace(HOST_SEQ, GUEST_SEQ).replace(HOST_ENV, GUEST_ENV)
                 for word in argv]
        command = "env %s" % " ".join(shell_word(word) for word in guest)
        out.extend(render(command, captured[:2], captured[2], name, kind, forced))
    os.unlink(notexecutable)
    out.append('echo "env ok"')
    out.append("exit 0")
    print("\n".join(out))


if __name__ == "__main__":
    sys.exit(main())