//! Testcase `bits.stdin`: groups from stdin lines are OR'ed together.
use std::process::ExitCode;
use bits_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "bits.stdin",
        args: vec!["--mask".to_string()],
        stdin: b"4,5-8\n16,30\n".to_vec(),
        want_stdout: b"0x400101f0\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
