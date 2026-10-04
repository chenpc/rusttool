//! Testcase `col.spaces` (guest id `col.spaces`): `-x` turns off the
//! default tab compression, so the tab in `a\tb` is expanded to spaces.

use std::process::ExitCode;

use col_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "col.spaces",
        args: vec!["-x".to_string()],
        stdin: b"a\tb\n".to_vec(),
        want_stdout: b"a       b\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
