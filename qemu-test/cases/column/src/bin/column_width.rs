//! Testcase `column.width` (guest id `column.width`): `-c 20` with seven
//! entries. 20 cells divided by the rounded-up 8 gives two columns, so the
//! input is read down the columns: a,e then bb,ff and so on.

use std::process::ExitCode;

use column_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "column.width",
        args: vec!["-c".to_string(), "20".to_string()],
        stdin: b"a\nbb\nccc\ndddd\ne\nff\nggg\n".to_vec(),
        want_stdout: b"a\te\nbb\tff\nccc\tggg\ndddd\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
