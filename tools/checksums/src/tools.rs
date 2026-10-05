//! The nine tool descriptions. Each one is the option set the real binary accepts,
//! in the order it lists them, because that order is what an ambiguous abbreviation
//! reports.

use hashes::Algorithm;

use crate::{Accepts, Tool};

const CHECK_ONLY: [&str; 5] = ["--ignore-missing", "--quiet", "--status", "--strict", "--warn"];

/// The hash tools all take the same options; b2sum is the one that also takes -l.
fn hash_tool(name: &'static str, algorithm: Algorithm, length: bool) -> Tool {
    let mut long = vec!["--binary", "--check"];
    if length {
        long.push("--length");
    }
    long.extend(["--tag", "--text", "--zero"]);
    long.extend(CHECK_ONLY);
    long.extend(["--help", "--version"]);
    Tool {
        name,
        default_algorithm: algorithm,
        default_tag: false,
        accepts: Accepts {
            check: true,
            tag: true,
            untagged: false,
            base64: false,
            raw: false,
            zero: true,
            length,
            binary: true,
            text: true,
            algorithm: false,
            check_only: true,
            debug: false,
        },
        short_opts: if length { "bcltwz" } else { "bctwz" },
        long_opts: Box::leak(long.into_boxed_slice()),
    }
}

pub fn md5sum() -> Tool {
    hash_tool("md5sum", Algorithm::Md5, false)
}
pub fn sha1sum() -> Tool {
    hash_tool("sha1sum", Algorithm::Sha1, false)
}
pub fn sha224sum() -> Tool {
    hash_tool("sha224sum", Algorithm::Sha224, false)
}
pub fn sha256sum() -> Tool {
    hash_tool("sha256sum", Algorithm::Sha256, false)
}
pub fn sha384sum() -> Tool {
    hash_tool("sha384sum", Algorithm::Sha384, false)
}
pub fn sha512sum() -> Tool {
    hash_tool("sha512sum", Algorithm::Sha512, false)
}
pub fn b2sum() -> Tool {
    hash_tool("b2sum", Algorithm::Blake2b(512), true)
}

pub fn cksum() -> Tool {
    let long = vec![
        "--algorithm",
        "--base64",
        "--check",
        "--length",
        "--raw",
        "--tag",
        "--untagged",
        "--zero",
        "--ignore-missing",
        "--quiet",
        "--status",
        "--strict",
        "--warn",
        "--debug",
        "--help",
        "--version",
    ];
    Tool {
        name: "cksum",
        default_algorithm: Algorithm::Crc,
        default_tag: true,
        accepts: Accepts {
            check: true,
            tag: true,
            untagged: true,
            base64: true,
            raw: true,
            zero: true,
            length: true,
            binary: false,
            text: false,
            algorithm: true,
            check_only: true,
            debug: true,
        },
        short_opts: "aclwz",
        long_opts: Box::leak(long.into_boxed_slice()),
    }
}

/// `sum` is the odd one out: no check mode, no tag, two options.
pub fn sum() -> Tool {
    Tool {
        name: "sum",
        default_algorithm: Algorithm::Bsd,
        default_tag: false,
        accepts: Accepts {
            check: false,
            tag: false,
            untagged: false,
            base64: false,
            raw: false,
            zero: false,
            length: false,
            binary: false,
            text: false,
            algorithm: false,
            check_only: false,
            debug: false,
        },
        short_opts: "rs",
        long_opts: Box::leak(vec!["--sysv", "--help", "--version"].into_boxed_slice()),
    }
}
