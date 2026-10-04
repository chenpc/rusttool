//! Testcase `line.empty`: empty stdin prints a newline, exit 1.
use std::process::ExitCode;
use line_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "line.empty",
        args: vec![],
        stdin: Vec::new(),
        want_stdout: b"\n".to_vec(),
        want_code: 1,
        want_stderr_contains: None,
    })
}
