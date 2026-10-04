//! Testcase `colrm.start` (guest id `colrm.start`): with only a start
//! column, everything from that column to the end of the line goes.

use std::process::ExitCode;

use colrm_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "colrm.start",
        args: vec!["3".to_string()],
        stdin: b"abcdef\n".to_vec(),
        want_stdout: b"ab\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
