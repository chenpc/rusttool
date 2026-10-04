//! Testcase `col.version` (guest id `col.version`): `-V` prints one version
//! line on stdout and exits 0, matching util-linux' `print_version()` format.

use std::process::ExitCode;

use col_cases::{run, Case};

/// The util-linux release the clone under test claims to track; keep in sync
/// with `UTIL_LINUX_VERSION` in `tools/col/src/lib.rs`.
const VERSION: &str = "2.39.3";

fn main() -> ExitCode {
    run(Case {
        name: "col.version",
        args: vec!["-V".to_string()],
        stdin: Vec::new(),
        want_stdout: format!("col from util-linux {}\n", VERSION).into_bytes(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
