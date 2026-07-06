use std::fs;
use std::path::Path;

const TEST_ARTIFACTS: &[&str] = &[
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

pub fn clean() {
    let removed = TEST_ARTIFACTS.iter().filter_map(remove_artifact).count();
    print_clean_summary(removed);
}

fn remove_artifact(name: &&str) -> Option<()> {
    let path = Path::new(name);
    if !path.exists() {
        return None;
    }
    match fs::remove_file(path) {
        Ok(_) => {
            println!("Removed: {name}");
            Some(())
        }
        Err(e) => {
            eprintln!("Failed to remove {name}: {e}");
            None
        }
    }
}

fn print_clean_summary(removed: usize) {
    if removed == 0 {
        println!("Nothing to clean.");
    } else {
        println!("Cleaned {removed} test artifact(s).");
    }
}