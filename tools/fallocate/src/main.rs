// fallocate - preallocate file space (simplified via set_len)
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: fallocate <file>");
        return;
    }
    let path = &args[0];
    match fs::File::open(path) {
        Ok(mut f) => {
            // Get current size and extend to 1GB
            let meta = f.metadata().unwrap();
            let len = meta.len();
            if len < 1_000_000_000 {
                if let Err(e) = f.set_len(1_000_000_000) {
                    eprintln!("fallocate: {}", e);
                } else {
                    println!("fallocate: OK");
                }
            }
        }
        Err(e) => {
            eprintln!("fallocate: {}", e);
        }
    }
}
