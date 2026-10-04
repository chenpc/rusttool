//! Testcase `colrm.tabs` (guest id `colrm.tabs`): columns are counted in
//! display positions, so a tab advances to the next multiple of 8. Removing
//! columns 3-5 from `a\tb\tc` leaves the tab partly blanked: `a` plus four
//! blanks plus `b\tc`.

use std::process::ExitCode;

use colrm_cases::{run, Case};

fn main() -> ExitCode {
    run(Case {
        name: "colrm.tabs",
        args: vec!["3".to_string(), "5".to_string()],
        stdin: b"a\tb\tc\n".to_vec(),
        want_stdout: b"a    b\tc\n".to_vec(),
        want_code: 0,
        want_stderr_contains: None,
    })
}
