//! `cat(1)`: concatenate files to standard output.

use std::io::{Read, Write};

use cat::{run, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: cat [OPTION]... [FILE]...

Concatenate FILE(s) to standard output.

  -A, --show-all              equivalent to -vET
  -b, --number-nonblank       number all non-empty lines
  -e, --show-ends             display end of lines as $
  -E, --show-endz             display end of lines as $
  -n, --number                number all output lines
  -s, --squeeze-blank         squeeze repeated blank output lines
  -t, --show-tabs             display tabs as ^I
  -T, --show-tabs             display tabs as ^I
  -u, --unbuffered            unbuffered write to stdout
  -v, --show-nonprinting      use ^ and M- notation, except LF and TAB
      --help     display this help and exit
      --version  output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') {
            files.push(arg);
            continue;
        }
        if arg == "--" {
            no_more = true;
            continue;
        }
        // Each option in the cluster is applied in turn.
        let flags: Vec<char> = if let Some(long) = arg.strip_prefix("--") {
            match long {
                "help" => {
                    print!("{}", HELP);
                    return;
                }
                "version" => {
                    println!("cat (rusttool) {}", VERSION);
                    return;
                }
                name => {
                    let flag = match name {
                        "show-all" => 'A',
                        "number-nonblank" => 'b',
                        "show-ends" | "show-endz" => 'e',
                        "number" => 'n',
                        "squeeze-blank" => 's',
                        "show-tabs" => 't',
                        "unbuffered" => 'u',
                        "show-nonprinting" => 'v',
                        other => {
                            eprintln!("cat: unrecognized option '--{}'", other);
                            eprintln!("Try 'cat --help' for more information.");
                            std::process::exit(1);
                        }
                    };
                    vec![flag]
                }
            }
        } else {
            let mut cluster: Vec<char> = Vec::new();
            for ch in arg[1..].chars() {
                if !matches!(ch, 'A' | 'b' | 'e' | 'E' | 'n' | 's' | 't' | 'T' | 'u' | 'v') {
                    eprintln!("cat: invalid option -- '{}'", ch);
                    eprintln!("Try 'cat --help' for more information.");
                    std::process::exit(1);
                }
                cluster.push(ch);
            }
            cluster
        };
        for flag in flags {
            match flag {
                'A' => options.show_all = true,
                'b' => options.number_nonblank = true,
                'e' | 'E' => options.show_line_end = true,
                'n' => options.number = true,
                's' => options.squeeze_blank = true,
                't' | 'T' => options.show_tabs = true,
                'u' => options.unbuffered = true,
                'v' => options.show_nonprinting = true,
                _ => {}
            }
        }
    }
    options.normalize();

    let mut out = Vec::new();
    let status = std::io::stdout().lock();
    let mut sink = status;
    let mut failed = false;

    let sources: Vec<Option<String>> = if files.is_empty() {
        vec![None]
    } else {
        files
            .iter()
            .map(|f| if f == "-" { None } else { Some(f.clone()) })
            .collect()
    };

    for source in sources {
        let mut data = Vec::new();
        match source {
            None => {
                if let Err(e) = std::io::stdin().read_to_end(&mut data) {
                    eprintln!("cat: -: {}", e);
                    failed = true;
                    continue;
                }
            }
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => data = bytes,
                Err(e) => {
                    eprintln!("cat: {}: {}", path, e);
                    failed = true;
                    continue;
                }
            },
        }
        run(&data, &options, &mut out);
        let _ = sink.write_all(&out);
        out.clear();
    }
    let _ = sink.flush();
    if failed {
        std::process::exit(1);
    }
}
