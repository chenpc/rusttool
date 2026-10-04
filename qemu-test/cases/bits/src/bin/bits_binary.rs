//! Testcase `bits.binary`: `--binary 4,5-8 16,30` (man example).
use std::process::ExitCode;
use bits_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "bits.binary",
        args: vec!["--binary".to_string(), "4,5-8".to_string(), "16,30".to_string()],
        stdin: Vec::new(),
        want_stdout: b"0b100_0000_0000_0001_0000_0001_1111_0000\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
