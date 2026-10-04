//! Testcase `column.separators` (guest id `column.separators`): `-t -s ,`
//! keeps empty fields, so `a,,b` spans three columns with a blank middle.

use std::process::ExitCode;

use column_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "column.separators",
        args: vec!["-t".to_string(), "-s".to_string(), ",".to_string()],
        stdin: b"a,,b\n".to_vec(),
        want_stdout: b"a    b\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
