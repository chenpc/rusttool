//! Testcase `rev.version` (guest id `rev.version`): `-V` prints one version
//! line on stdout and exits 0, matching util-linux' `print_version()` format.

use std::process::ExitCode;

use rev_cases::{run, Case};

/// The util-linux release the clone under test claims to track; keep in sync
/// with `UTIL_LINUX_VERSION` in `tools/rev/src/lib.rs`.
const VERSION: &str = "2.42";

fn main() -> ExitCode {
    run(Case {
        name: "rev.version",
        args: vec!["-V".to_string()],
        stdin: Vec::new(),
        want_stdout: format!("rev from util-linux {}\n", VERSION).into_bytes(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
