//! ディレクトリ内の HEIC/HEIF ファイルを JPEG に一括変換する。
//!
//! ## Usage
//!
//! ```sh
//! cargo run --example convert_dir -- <input_dir> <output_dir>
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input_dir> <output_dir>", args[0]);
        return ExitCode::FAILURE;
    }

    let input_dir = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);

    if !input_dir.is_dir() {
        eprintln!(
            "Error: input directory does not exist: {}",
            input_dir.display()
        );
        return ExitCode::FAILURE;
    }

    if !output_dir.exists()
        && let Err(e) = std::fs::create_dir_all(&output_dir)
    {
        eprintln!("Error: failed to create output directory: {}", e);
        return ExitCode::FAILURE;
    }

    let heic_extensions = [".heic", ".heif"];
    let mut total = 0u32;
    let mut success = 0u32;
    let mut failed = 0u32;

    let entries = match std::fs::read_dir(&input_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Error: failed to read input directory: {}", e);
            return ExitCode::FAILURE;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let lower = filename.to_lowercase();
        if !heic_extensions.iter().any(|ext| lower.ends_with(ext)) {
            continue;
        }

        total += 1;

        let heic_data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("FAIL: {} - failed to read: {}", filename, e);
                failed += 1;
                continue;
            }
        };

        match heic_converter::convert_heic_to_jpeg(&heic_data) {
            Ok(jpeg_data) => {
                let output_filename = replace_extension(filename, "jpg");
                let output_path = output_dir.join(&output_filename);
                match std::fs::write(&output_path, &jpeg_data) {
                    Ok(()) => {
                        println!(
                            "OK: {} -> {} ({} bytes)",
                            filename,
                            output_filename,
                            jpeg_data.len()
                        );
                        success += 1;
                    }
                    Err(e) => {
                        eprintln!("FAIL: {} - failed to write output: {}", filename, e);
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("FAIL: {} - conversion error: {}", filename, e);
                failed += 1;
            }
        }
    }

    println!();
    println!("Total: {}, Success: {}, Failed: {}", total, success, failed);

    if total == 0 {
        eprintln!("Error: no HEIC/HEIF files found in {}", input_dir.display());
        return ExitCode::FAILURE;
    }

    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn replace_extension(filename: &str, new_ext: &str) -> String {
    if let Some(pos) = filename.rfind('.') {
        format!("{}.{}", &filename[..pos], new_ext)
    } else {
        format!("{}.{}", filename, new_ext)
    }
}
