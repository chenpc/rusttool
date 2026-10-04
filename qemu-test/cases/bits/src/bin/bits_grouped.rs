//! Testcase `bits.grouped`: `--grouped-mask 2,22,74,79` (man example).
use std::process::ExitCode;
use bits_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "bits.grouped",
        args: vec!["--grouped-mask".to_string(), "2,22,74,79".to_string()],
        stdin: Vec::new(),
        want_stdout: b"8400,00000000,00400004\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
