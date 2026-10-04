//! Testcase `line.basic`: first line echoed, rest ignored, exit 0.
use std::process::ExitCode;
use line_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "line.basic",
        args: vec![],
        stdin: b"hello\nrest\n".to_vec(),
        want_stdout: b"hello\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
