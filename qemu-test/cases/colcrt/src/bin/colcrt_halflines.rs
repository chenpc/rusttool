//! Testcase `colcrt.halflines`: `-2` adds the half-line blank lines.
use std::process::ExitCode;
use colcrt_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "colcrt.halflines",
        args: vec!["-2".to_string()],
        stdin: b"test\n".to_vec(),
        want_stdout: b"\ntest\n\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
