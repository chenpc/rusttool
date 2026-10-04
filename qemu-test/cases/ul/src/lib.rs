//! Tiny shared harness for the guest-side testcase binaries of one tool.
//!
//! Every case binary is a standalone static musl executable that the guest
//! `/init` execs as `/mnt/tests/<tool>.<case>`. There is no shell in the guest,
//! so the cases spawn the tool under test with [`std::process::Command`] and
//! compare bytes directly. `ExitCode::SUCCESS` (0) is the PASS signal `/init`
//! turns into the `QEMU-TEST: PASS` serial marker.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The tool under test, installed by pack-initramfs.sh on the 9p share.
pub const TOOL: &str = "/mnt/bin/ul";

/// Read-only 9p share inside the guest.
const SHARE: &str = "/mnt/tests";

/// One testcase: run `TOOL` with `args`/`stdin`, compare the result.
///
/// `want_stdout` and `want_code` are exact; `want_stderr_contains` is a
/// substring check, because `std`'s rendering of an OS error
/// ("No such file or directory (os error 2)") is deliberately not part of the
/// contract the C original defines with `strerror(3)`.
pub struct Case {
    pub name: &'static str,
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    pub want_stdout: Vec<u8>,
    pub want_code: i32,
    pub want_stderr_contains: Option<&'static str>,
}

/// Absolute path of a fixture shipped in `cases/<tool>/fixtures/`.
pub fn fixture(name: &str) -> String {
    let mut path = PathBuf::from(SHARE);
    path.push("fixtures");
    path.push(tool_name());
    path.push(name);
    path.to_string_lossy().into_owned()
}

/// Directory component of the test id, i.e. the tool name.
fn tool_name() -> &'static str {
    Path::new(TOOL)
        .file_name()
        .expect("TOOL has a file name")
        .to_str()
        .expect("TOOL is UTF-8")
}

/// Printable form of arbitrary bytes, so a mismatch is readable on the serial line.
fn show(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');
    for &byte in bytes {
        match byte {
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(byte as char),
            other => out.push_str(&format!("\\x{:02x}", other)),
        }
    }
    out.push('"');
    out
}

/// Run `case` and translate the verdict into an exit code.
pub fn run(case: Case) -> ExitCode {
    let problems = check(&case);
    if problems.is_empty() {
        return ExitCode::SUCCESS;
    }
    for problem in problems {
        eprintln!("{}: {}", case.name, problem);
    }
    ExitCode::from(1)
}

/// Same as [`run`], but returns the problems instead of reporting them, for the
/// rare case that needs to aggregate several checks.
pub fn check(case: &Case) -> Vec<String> {
    let mut problems = Vec::new();

    let mut command = Command::new(TOOL);
    command
        .args(&case.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            problems.push(format!("spawn {}: {}", TOOL, error));
            return problems;
        }
    };

    if let Some(mut pipe) = child.stdin.take() {
        use std::io::Write;
        if let Err(error) = pipe.write_all(&case.stdin).and_then(|()| pipe.flush()) {
            problems.push(format!("write stdin: {}", error));
        }
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(error) => {
            problems.push(format!("wait {}: {}", TOOL, error));
            return problems;
        }
    };

    if output.stdout != case.want_stdout {
        problems.push(format!(
            "stdout: want {} got {}",
            show(&case.want_stdout),
            show(&output.stdout)
        ));
    }
    if output.status.code() != Some(case.want_code) {
        problems.push(format!(
            "exit code: want {} got {:?}{}",
            case.want_code,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if let Some(needle) = case.want_stderr_contains {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains(needle) {
            problems.push(format!(
                "stderr: want a message containing {:?}, got {:?}",
                needle,
                stderr
            ));
        }
    }
    problems
}
