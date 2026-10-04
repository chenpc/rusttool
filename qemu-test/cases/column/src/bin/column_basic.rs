//! Testcase `column.basic` (guest id `column.basic`): the default FillCols
//! layout. The longest entry is 4 cells, rounded up to one 8-cell tab stop,
//! so all four entries share one 80-cell row joined by tabs.

use std::process::ExitCode;

use column_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "column.basic",
        args: vec![],
        stdin: b"a\nbb\nccc\ndddd\n".to_vec(),
        want_stdout: b"a\tbb\tccc\tdddd\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
