use file_tree_json::{build_tree, TraversalMode};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

fn print_usage() {
    eprintln!("Usage: file_tree_json [OPTIONS] [PATH] [--output FILE]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --flat, -f             Output as flat JSON array");
    eprintln!("  --tree, -t             Output as nested tree structure (default)");
    eprintln!("  --output, -o FILE      Write output to FILE instead of stdout");
    eprintln!("  --force-canonical, -fc Force the absolute representation of the path");
    eprintln!("  --help, -h             Show this help message");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  PATH  Directory path to scan (default: current directory)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut mode = "tree";
    let mut output_file: Option<String> = None;
    let mut path_index = 1;
    let mut force_canonical = false;

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
            "--force-canonical" | "-fc" => {
                force_canonical = true;
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

    let start = Instant::now();
    let tree = build_tree(Path::new(path), TraversalMode::Parallel, force_canonical).unwrap();
    eprintln!("DEBUG: build_tree took {:?} - {} files, {} directories", start.elapsed(), tree.files_nb, tree.dirs_nb);

    let json_result = match mode {
        // "flat" => {
        //     let start_flat = Instant::now();
        //     // let flat_nodes = tree_to_flat(&tree);
        //     // eprintln!("DEBUG: tree_to_flat took {:?} for {} nodes", start_flat.elapsed(), flat_nodes.len());
        //     eprintln!("DEBUG: tree_to_flat took {:?} for {} nodes", start_flat.elapsed(), flat_nodes.len());

        //     // serde_json::to_string_pretty(&flat_nodes)
        // }
        "tree" | _ => {
            serde_json::to_string_pretty(&tree)
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