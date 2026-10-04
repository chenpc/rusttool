//! Testcase `rev.missing` (guest id `rev.missing`): a nonexistent operand.
//! Upstream warns, keeps processing the remaining operands, and exits
//! `EXIT_FAILURE` (1) at the end.

use std::process::ExitCode;

use rev_cases::{fixture, run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "rev.missing",
        args: vec![fixture("does-not-exist")],
        stdin: Vec::new(),
        want_stdout: Vec::new(),
        want_code: 1,
        want_stderr_contains: Some("cannot open"),
    })
}
