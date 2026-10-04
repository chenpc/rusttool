//! Testcase `col.overstrike` (guest id `col.overstrike`): `abc` followed by
//! two backspaces and `Z`. Without `-b` upstream keeps the overwritten column
//! as a backspace pair, emitting `ab`, backspace, `Z`, `c`.

use std::process::ExitCode;

use col_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "col.overstrike",
        args: vec![],
        stdin: b"abc\x08\x08Z\n".to_vec(),
        want_stdout: b"ab\x08Zc\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
