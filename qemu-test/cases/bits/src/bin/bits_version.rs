//! Testcase `bits.version`: `-V` prints the version line.
use std::process::ExitCode;
use bits_cases::{run, Case};
const VERSION: &str = "2.42";
fn main() -> ExitCode {
    run(Case {
        name: "bits.version",
        args: vec!["-V".to_string()],
        stdin: Vec::new(),
        want_stdout: format!("bits from util-linux {}\n", VERSION).into_bytes(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
