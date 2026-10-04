//! Testcase `col.badusage` (guest id `col.badusage`): `col` reads only
//! standard input, so any file operand is a usage error with a `bad usage`
//! warning on stderr and exit status 1.

use std::process::ExitCode;

use col_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "col.badusage",
        args: vec!["foo".to_string()],
        stdin: Vec::new(),
        want_stdout: Vec::new(),
        want_code: 1,
        want_stderr_contains: Some("bad usage"),
    })
}
