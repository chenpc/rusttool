//! Testcase `bits.list`: `--list 0xeec2` compresses to ranges (man example).
use std::process::ExitCode;
use bits_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "bits.list",
        args: vec!["--list".to_string(), "0xeec2".to_string()],
        stdin: Vec::new(),
        want_stdout: b"1,6,7,9-11,13-15\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
