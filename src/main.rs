use file_tree_json::{build_flat_tree, build_tree};
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn print_usage() {
    eprintln!("Usage: file_tree_json [OPTIONS] [PATH] [--output FILE]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --flat, -f          Output as flat JSON array");
    eprintln!("  --tree, -t          Output as nested tree structure (default)");
    eprintln!("  --output, -o FILE  Write output to FILE instead of stdout");
    eprintln!("  --help, -h         Show this help message");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  PATH  Directory path to scan (default: current directory)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut mode = "tree";
    let mut output_file: Option<String> = None;
    let mut path_index = 1;

    while path_index < args.len() {
        match args[path_index].as_str() {
            "--flat" | "-f" => {
                mode = "flat";
                path_index += 1;
            }
            "--tree" | "-t" => {
                mode = "tree";
                path_index += 1;
            }
            "--output" | "-o" => {
                path_index += 1;
                if path_index >= args.len() {
                    eprintln!("Error: --output requires a FILE argument");
                    print_usage();
                    std::process::exit(1);
                }
                output_file = Some(args[path_index].clone());
                path_index += 1;
            }
            "--help" | "-h" => {
                print_usage();
                return;
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown option: {}", arg);
                print_usage();
                std::process::exit(1);
            }
            _ => break,
        }
    }

    let path = if path_index < args.len() {
        &args[path_index]
    } else {
        "."
    };

    let json_result = match mode {
        "flat" => {
            match build_flat_tree(Path::new(path)) {
                Ok(nodes) => serde_json::to_string_pretty(&nodes),
                Err(e) => {
                    eprintln!("Erreur: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "tree" | _ => {
            match build_tree(Path::new(path)) {
                Ok(tree) => serde_json::to_string_pretty(&tree),
                Err(e) => {
                    eprintln!("Erreur: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    match json_result {
        Ok(json) => {
            match output_file {
                Some(file_path) => {
                    if let Err(e) = File::create(&file_path).and_then(|mut f| f.write_all(json.as_bytes())) {
                        eprintln!("Erreur: impossible d'écrire dans le fichier {}: {}", file_path, e);
                        std::process::exit(1);
                    }
                }
                None => {
                    println!("{}", json);
                }
            }
        }
        Err(e) => {
            eprintln!("Erreur: {}", e);
            std::process::exit(1);
        }
    }
}