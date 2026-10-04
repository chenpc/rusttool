//! Testcase `column.table` (guest id `column.table`): `-t` with a short
//! final row. Upstream still pads the cells the last row has and emits the
//! separators of the cells it does not, so `1` keeps five trailing blanks.

use std::process::ExitCode;

use column_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "column.table",
        args: vec!["-t".to_string()],
        stdin: b"a b c\n1\n".to_vec(),
        want_stdout: b"a  b  c\n1     \n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
