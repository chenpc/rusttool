//! Testcase `colcrt.file`: file operand is filtered like stdin.
use std::process::ExitCode;
use colcrt_cases::{fixture, run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "colcrt.file",
        args: vec![fixture("hello.txt")],
        stdin: Vec::new(),
        want_stdout: b"hello\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
