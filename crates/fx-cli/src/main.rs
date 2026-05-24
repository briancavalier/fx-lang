use anyhow::{Context, Result, bail};
use fx_core::{SourceFile, check_workspace, index_workspace};
use fx_syntax::parse_file;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        bail!("missing command");
    };
    match command.as_str() {
        "parse" => {
            let file = required_path(args.next(), "parse")?;
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read `{}`", file.display()))?;
            let parsed = parse_file(source);
            print_json(&parsed)?;
        }
        "symbols" => {
            let root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
            let files = collect_files(&root)?;
            let graph = index_workspace(&root, files);
            print_json(&graph)?;
        }
        "check" => {
            let root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
            let files = collect_files(&root)?;
            let graph = index_workspace(&root, files);
            let result = check_workspace(&graph);
            print_json(&result)?;
        }
        _ => {
            usage();
            bail!("unknown command `{command}`");
        }
    }
    Ok(())
}

fn required_path(arg: Option<String>, command: &str) -> Result<PathBuf> {
    arg.map(PathBuf::from)
        .with_context(|| format!("`{command}` requires a file path"))
}

fn collect_files(root: &Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "fx") {
            files.push(SourceFile {
                path: entry.path().to_path_buf(),
                source: fs::read_to_string(entry.path())
                    .with_context(|| format!("failed to read `{}`", entry.path().display()))?,
            });
        }
    }
    Ok(files)
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  fx parse <file.fx>");
    eprintln!("  fx symbols [root]");
    eprintln!("  fx check [root]");
}
