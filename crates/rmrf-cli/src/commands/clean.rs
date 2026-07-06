use std::fs;
use std::path::Path;

pub fn clean() {
    let artifacts = [
        "empty",
        "one_byte",
        "zeros",
        "ff_bytes",
        "repeated",
        "exact_chunk",
        "chunk_plus",
        "restored",
        "rmrf_src",
        "test.txt",
        "input.txt",
        "input.bin",
        "rmrf_input.txt",
        "-rmrf_input.txt",
        "écoß测试.txt",
    ];

    let mut removed = 0;
    for name in &artifacts {
        let path = Path::new(name);
        if path.exists() {
            match fs::remove_file(path) {
                Ok(_) => {
                    println!("Removed: {name}");
                    removed += 1;
                }
                Err(e) => eprintln!("Failed to remove {name}: {e}"),
            }
        }
    }

    if removed == 0 {
        println!("Nothing to clean.");
    } else {
        println!("Cleaned {removed} test artifact(s).");
    }
}