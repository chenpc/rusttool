// dmesg - print kernel ring buffer (via /dev/kmsg)
use std::fs;
use std::io::Read;

fn main() {
    match fs::File::open("/dev/kmsg") {
        Ok(mut f) => {
            let mut buf = Vec::new();
            if let Err(e) = f.read_to_end(&mut buf) {
                eprintln!("dmesg: {}", e);
                return;
            }
            // /dev/kmsg returns null-separated entries
            let mut start = 0;
            while start < buf.len() {
                match buf[start..].iter().position(|c| *c == b'\0') {
                    Some(pos) => {
                        let end = start + pos;
                        let entry = String::from_utf8_lossy(&buf[start..end]);
                        println!("{}", entry.trim_end());
                        start = end + 1;
                    }
                    None => break,
                }
            }
        }
        Err(_) => match fs::File::open("/proc/kmsg") {
            Ok(mut f) => {
                let mut buf = Vec::new();
                if let Err(e) = f.read_to_end(&mut buf) {
                    eprintln!("dmesg: {}", e);
                    return;
                }
                let mut start = 0;
                while start < buf.len() {
                    match buf[start..].iter().position(|c| *c == b'\0') {
                        Some(pos) => {
                            let end = start + pos;
                            let entry = String::from_utf8_lossy(&buf[start..end]);
                            println!("{}", entry.trim_end());
                            start = end + 1;
                        }
                        None => break,
                    }
                }
            }
            Err(e) => {
                eprintln!("dmesg: {}", e);
            }
        },
    }
}
