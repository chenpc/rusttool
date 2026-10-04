//! Testcase `rev.nonewline` (guest id `rev.nonewline`): the last line has no
//! trailing newline. util-linux rev(1) still reverses it and emits it without
//! adding a separator (the 1994 patch in the upstream history).

use std::process::ExitCode;

use rev_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "rev.nonewline",
        args: vec![],
        stdin: b"abc\nno newline".to_vec(),
        want_stdout: b"cba\nenilwen on".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
