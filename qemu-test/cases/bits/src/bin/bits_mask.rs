//! Testcase `bits.mask`: `--mask 4,5-8 16,30` (man example -> 0x400101f0).
use std::process::ExitCode;
use bits_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "bits.mask",
        args: vec!["--mask".to_string(), "4,5-8".to_string(), "16,30".to_string()],
        stdin: Vec::new(),
        want_stdout: b"0x400101f0\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
