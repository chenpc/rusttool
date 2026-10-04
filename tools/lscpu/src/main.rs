fn main() {
    let data = match std::fs::read_to_string("/proc/cpuinfo") {
        Ok(d) => d,
        Err(e) => { eprintln!("lscpu: {}", e); return; }
    };

    let mut vendor = String::new();
    let mut model = String::new();
    let mut cores = 0u32;
    let mut threads = 0u32;
    let mut flags: Vec<String> = vec![];

    for line in data.lines() {
        if let Some(v) = line.strip_prefix("vendor_id:\t") {
            vendor = v.trim().to_string();
        } else if let Some(m) = line.strip_prefix("model:\t") {
            model = m.trim().to_string();
        } else if let Some(c) = line.strip_prefix("cpu cores:\t") {
            cores = c.trim().parse().unwrap_or(0);
        } else if let Some(t) = line.strip_prefix("threads:\t") {
            threads = t.trim().parse().unwrap_or(0);
        } else if let Some(f) = line.strip_prefix("flags:\t") {
            flags = f.trim().split_whitespace().map(String::from).collect();
        }
    }

    println!("Vendor:       {}", vendor);
    println!("Model:        {}", model);
    println!("CPU cores:    {}", cores);
    println!("Threads:      {}", threads);
    println!("Flags:");
    for f in flags {
        println!("  {}", f);
    }
}
