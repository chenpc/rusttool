//! Testcase `colcrt.nounder`: lone `-` suppresses the underline line.
use std::process::ExitCode;
use colcrt_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "colcrt.nounder",
        args: vec!["-".to_string()],
        stdin: b"A_\n".to_vec(),
        want_stdout: b"A\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
