//! Testcase `line.version`: `-V` prints the version line.
use std::process::ExitCode;
use line_cases::{run, Case};
const VERSION: &str = "2.42";
fn main() -> ExitCode {
    run(Case {
        name: "line.version",
        args: vec!["-V".to_string()],
        stdin: Vec::new(),
        want_stdout: format!("line from util-linux {}\n", VERSION).into_bytes(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
