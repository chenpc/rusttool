//! Testcase `column.files` (guest id `column.files`): two fixture files as
//! operands. Upstream processes operands in order and concatenates the
//! entries, so this also pins the operand order.

use std::process::ExitCode;

use column_cases::{fixture, run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "column.files",
        args: vec![fixture("f1.txt"), fixture("f2.txt")],
        stdin: Vec::new(),
        // f1.txt is "one\ntwo\n" and f2.txt is "three\n": three entries in
        // one 80-cell row, joined by tabs.
        want_stdout: b"one\ttwo\tthree\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
