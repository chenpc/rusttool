//! Testcase `colrm.badop` (guest id `colrm.badop`): a non-numeric operand is
//! rejected on stderr with exit status 1.

use std::process::ExitCode;

use colrm_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "colrm.badop",
        args: vec!["x".to_string()],
        stdin: b"abcdef\n".to_vec(),
        want_stdout: Vec::new(),
        want_code: 1,
        want_stderr_contains: Some("first argument"),
    })
}
