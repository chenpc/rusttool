//! Testcase `colcrt.basic`: plain text passes through unchanged.
use std::process::ExitCode;
use colcrt_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "colcrt.basic",
        args: vec![],
        stdin: b"hello\nworld\n".to_vec(),
        want_stdout: b"hello\nworld\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
