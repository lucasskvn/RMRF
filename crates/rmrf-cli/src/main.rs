/*
** EPITECH PROJECT, 2026
** RMRF
** File description:
** main.rs
*/

use rmrf_core::{ChunkStore, Source};
use rmrf_source::FileSource;
use rmrf_store::{LocalChunkStore, hash_data};
use std::env;
use std::fs;
use std::path::PathBuf;

const CHUNK_SIZE: usize = 8192;

fn main()
{
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Usage: rmrf <backup|restore> <path> [store_dir]");
        return;
    }
    let cmd = &args[1];
    let path = PathBuf::from(&args[2]);
    let store_dir = args.get(3).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("./rmrf-store"));

    match cmd.as_str() {
        "backup" => backup(&path, &store_dir),
        "restore" => restore(&path, &store_dir),
        _ => println!("Unknown command: {}", cmd),
    }
}

fn backup(source: &PathBuf, store_dir: &PathBuf)
{
    println!("Backup: {]", source.display());

    let mut file_source = match FileSource::new(source) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error opening file: {}", e);
            return;
        }
    };
    let metadata = file_source.metadata();
    println!("File size: {} bytes", metadata.size);
    let mut store = match LocalChunkStore::new(store_dir.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error creating store: {}", e);
            return;
        }
    };
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunk_count = 0;
    let mut total_bytes = 0;

    loop {
        match file_source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let data = &buffer[..n];
                let hash = hash_data(data);

                if let Err(e) = store.put(&hash, data) {
                    eprintln!("Error storing chunk: {}", e);
                    return;
                }
                chunk_count += 1;
                total_bytes += n as u64;
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            Err(e) => {
                eprintln!("Error reading file: {}", e);
                return;
            }
        }
    }
    println!("\nBackup complete!");
    println!("Chunks: {}", chunk_count);
    println!("Total bytes: {}", total_bytes);
    println!("Store: {}", store_dir.display());
}

fn restore(dest: &PathBuf, store_dir: &PathBuf)
{
    println!("Restore: {}", dest.display());
    println!("From store: {}", store_dir.display());

    match fs::read_dir(store_dir) {
        Ok(entries) => {
            let mut chunk_count = 0;
            for dir_entry in entries {
                if let Ok(entry) = dir_entry {
                    if entry.path().is_dir() {
                        if let Ok(files) = fs::read_dir(entry.path()) {
                            for file_entry in files {
                                if let Ok(_) = file_entry {
                                    chunk_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            println!("Found {} chunks to restore", chunk_count);
        }
        Err(e) => eprintln!("Error reading store: {}", e),
    }
}
