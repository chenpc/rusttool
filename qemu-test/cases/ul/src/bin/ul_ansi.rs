//! Testcase `ul.ansi`: a capable terminal gets ANSI underline sequences.
use std::process::ExitCode;
use ul_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "ul.ansi",
        args: vec!["-t".to_string(), "xterm".to_string()],
        stdin: b"_\x08a\n".to_vec(),
        want_stdout: b"\x1b[4ma\x1b[m\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
