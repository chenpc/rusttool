//! Testcase `colrm.basic` (guest id `colrm.basic`): remove the inclusive
//! column range 3-5, so `abcdefgh` becomes `abfgh`.

use std::process::ExitCode;

use colrm_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "colrm.basic",
        args: vec!["3".to_string(), "5".to_string()],
        stdin: b"abcdefgh\n".to_vec(),
        want_stdout: b"abfgh\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
