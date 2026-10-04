//! Testcase `ul.indicated`: `-i` appends the `_` marker line in dumb mode.
use std::process::ExitCode;
use ul_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "ul.indicated",
        args: vec!["-t".to_string(), "dumb".to_string(), "-i".to_string()],
        stdin: b"_\x08a\n".to_vec(),
        want_stdout: b"a\n_\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
