//! Testcase `column.fillrows` (guest id `column.fillrows`): `-x` walks the
//! same seven entries across the columns instead of down them.

use std::process::ExitCode;

use column_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "column.fillrows",
        args: vec!["-x".to_string(), "-c".to_string(), "20".to_string()],
        stdin: b"a\nbb\nccc\ndddd\ne\nff\nggg\n".to_vec(),
        want_stdout: b"a\tbb\nccc\tdddd\ne\tff\nggg\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
