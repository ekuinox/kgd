#[cfg(not(unix))]
fn main() {
    panic!("unsupported environment");
}

#[cfg(unix)]
fn main() {
    use std::{env, path::Path};

    use heif::heif_to_jpeg;

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.heic> [output.jpg]", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        let path = Path::new(input_path);
        let stem = path.file_stem().unwrap().to_str().unwrap();
        format!("{}.jpg", stem)
    };

    heif_to_jpeg(input_path, &output_path).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });

    println!("Converted {} -> {}", input_path, output_path);
}
