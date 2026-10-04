//! Testcase `colrm.noargs` (guest id `colrm.noargs`): with no operands
//! nothing is ever removed, so the tool degenerates to a verbatim copy,
//! tabs included.

use std::process::ExitCode;

use colrm_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "colrm.noargs",
        args: vec![],
        stdin: b"abcdef\n\tx\ty\n".to_vec(),
        want_stdout: b"abcdef\n\tx\ty\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
