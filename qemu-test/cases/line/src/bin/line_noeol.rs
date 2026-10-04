//! Testcase `line.noeol`: EOF-terminated input still printed, exit 1.
use std::process::ExitCode;
use line_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "line.noeol",
        args: vec![],
        stdin: b"partial".to_vec(),
        want_stdout: b"partial\n".to_vec(),
        want_code: 1,
        want_stderr_contains: None,
    })
}
