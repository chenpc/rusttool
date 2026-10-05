# rusttool — coreutils and util-linux in Rust

Each tool here is a from-scratch Rust implementation of one GNU tool, verified
three ways before it counts as done:

1. **unit tests** in the crate's own `lib.rs`, for the logic that does not need I/O
2. **differential** against the system binary — stdout, stderr and exit status
   compared byte for byte, on inputs generated to hit the awkward paths
3. **integration** on a freshly booted Alpine guest under QEMU, in a static musl
   build, with no `libc` beyond what the tool actually calls

The third layer is the reason this repository exists in the shape it does. A tool
that passes two layers can still be wrong in the only environment that matters.

## Where things are

| Path | What |
| --- | --- |
| `tools/<name>/` | one crate per tool; `lib.rs` is pure logic plus its tests, `main.rs` does I/O |
| `tools/quoting/` | the two quoting styles GNU tools use in diagnostics, shared |
| `qemu-test/` | the guest harness: kernel, initramfs, testcases, boot scripts |
| `qemu-test/cases/<tool>/` | Rust testcase crates for tools with no `/usr/bin` counterpart |
| `qemu-test/cases-shell/<tool>.*.sh` | shell testcases, with expectations generated from the system tool |
| `qemu-test/tmp/` | differential harnesses, fuzzers and scratch scripts |
| `HANDOVER.md` | what every tool turned out to be hiding, tool by tool |
| `TODO.md` | what is left, and the known differences that cannot be removed |

## Rules this repository holds itself to

**An expectation is never written by hand.** Every expected string in a testcase
comes from running the system tool and recording what it printed. A
hand-written expectation is how a test starts lying: it encodes what the author
believed, and the test then passes forever.

**A test that only runs on one platform is not a test.** The guest has a musl C
library and the host has glibc, and they disagree about several things that show
up in output. Those are written down in `TODO.md` rather than papered over.

**Known differences are recorded, not hidden.** Where byte-identical output is
not reachable, the case says so, names the reason, and says how it was measured.

## Building

```bash
cargo build --workspace          # the tools, for this machine
cargo test --workspace           # the unit tests
```

The guest build is static musl, which needs the target installed:

```bash
rustup target add x86_64-unknown-linux-musl
qemu-test/pack-initramfs.sh      # builds every tool, packs the initramfs
```

`qemu-test/ENV-SETUP.md` covers the kernel, the modules, and the scratch disks
the harness attaches for the tools that need a real block device.

## Running the verification

```bash
qemu-test/tmp/final-tally.sh     # unit tests, every differential, then the guest suite
```

Individual pieces:

```bash
qemu-test/tmp/<tool>-diff.sh     # one tool against /usr/bin/<tool>
python3 qemu-test/tmp/<tool>-fuzz.py    # random inputs, random option shapes
qemu-test/run-many.sh            # boot a guest per testcase
```

## What is not here yet

The guest and the host tool sets differ in size, and this repository covers the
smaller of the two. `TODO.md` lists what remains, ordered by which batch unblocks
the most: plain text tools first, then the privileged ones, which need the real
block devices the harness now attaches.