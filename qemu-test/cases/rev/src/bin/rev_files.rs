//! Testcase `rev.files` (guest id `rev.files`): two fixture files as operands.
//! Upstream processes operands in order and concatenates the results into one
//! stream, so this also pins the operand order.

use std::process::ExitCode;

use rev_cases::{fixture, run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "rev.files",
        args: vec![fixture("a.txt"), fixture("b.txt")],
        stdin: Vec::new(),
        // a.txt is "one\ntwo\n" and b.txt is "three\n".
        want_stdout: b"eno\nowt\neerht\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
