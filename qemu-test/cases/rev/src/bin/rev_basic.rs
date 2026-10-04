//! Testcase `rev.basic` (guest id `rev.basic`): stdin only, several lines
//! including a blank one. Upstream reverses every line and leaves blank lines
//! alone, and prints nothing but the reversed text on stdout.

use std::process::ExitCode;

use rev_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "rev.basic",
        args: vec![],
        // Note the blank second line and the single-character first line.
        stdin: b"one\n\nabc\nxyz\n".to_vec(),
        want_stdout: b"eno\n\ncba\nzyx\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
