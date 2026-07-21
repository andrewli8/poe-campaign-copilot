//! Simulates live play by appending fixture lines to a target file.
//! Usage: fake-play <fixture> <target-file> [delay-ms]

use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (fixture, target) = match (args.get(1), args.get(2)) {
        (Some(f), Some(t)) => (f.clone(), t.clone()),
        _ => {
            eprintln!("usage: fake-play <fixture> <target-file> [delay-ms]");
            std::process::exit(2);
        }
    };
    let delay_ms: u64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(300);

    let text = std::fs::read_to_string(&fixture)?;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)?;

    let total = text.lines().count();
    for (i, line) in text.lines().enumerate() {
        writeln!(out, "{line}")?;
        out.flush()?;
        println!("[{}/{total}] {line}", i + 1);
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
    println!("done: {total} lines appended to {target}");
    Ok(())
}
