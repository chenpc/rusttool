//! Testcase `ul.dumb`: `-t dumb` drops styling (underline + bold -> plain).
use std::process::ExitCode;
use ul_cases::{run, Case};
fn main() -> ExitCode {
    run(Case {
        name: "ul.dumb",
        args: vec!["-t".to_string(), "dumb".to_string()],
        stdin: b"_\x08a b\x08b\n".to_vec(),
        want_stdout: b"a b\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
