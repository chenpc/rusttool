//! Testcase `col.plain` (guest id `col.plain`): plain text passes through
//! untouched, including its trailing newline.

use std::process::ExitCode;

use col_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "col.plain",
        args: vec![],
        stdin: b"abc\n".to_vec(),
        want_stdout: b"abc\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
