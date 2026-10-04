//! Testcase `col.nobs` (guest id `col.nobs`): `-b` emits only the last
//! character of an overstruck column instead of the backspace pair, so the
//! same input as `col.overstrike` renders as plain `aZc`.

use std::process::ExitCode;

use col_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "col.nobs",
        args: vec!["-b".to_string()],
        stdin: b"abc\x08\x08Z\ny\x08W\n".to_vec(),
        want_stdout: b"aZc\nW\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
