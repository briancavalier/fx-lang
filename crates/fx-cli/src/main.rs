use anyhow::{Context, Result, bail};
use fx_compiler::{
    CompileOptions, CompileTarget, emit_javascript_modules, lower_to_core, prepare_compile,
};
use fx_core::{Analysis, AnalysisDiagnostic, SourceFile, analyze_workspace, index_workspace};
use fx_syntax::parse_file;
use fx_syntax::span::Span;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> Result<ExitCode> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        bail!("missing command");
    };
    let exit_code = match command.as_str() {
        "parse" => {
            let file = required_path(args.next(), "parse")?;
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read `{}`", file.display()))?;
            let parsed = parse_file(source);
            print_json(&parsed)?;
            ExitCode::SUCCESS
        }
        "symbols" => {
            let root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
            let files = collect_files(&root)?;
            let graph = index_workspace(&root, files);
            print_json(&graph)?;
            ExitCode::SUCCESS
        }
        "check" => {
            let options = CheckOptions::parse(args.collect())?;
            let output = check_output(&options)?;
            print!("{stdout}", stdout = output.stdout);
            eprint!("{stderr}", stderr = output.stderr);
            output.exit_code
        }
        "compile" => {
            let options = CompileCliOptions::parse(args.collect())?;
            let output = compile_output(&options)?;
            print!("{stdout}", stdout = output.stdout);
            eprint!("{stderr}", stderr = output.stderr);
            output.exit_code
        }
        _ => {
            usage();
            bail!("unknown command `{command}`");
        }
    };
    Ok(exit_code)
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

fn collect_check_files(path: &Path) -> Result<(PathBuf, Vec<SourceFile>)> {
    if path.is_file() {
        let root = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read `{}`", path.display()))?;
        return Ok((
            root,
            vec![SourceFile {
                path: path.to_path_buf(),
                source,
            }],
        ));
    }

    Ok((path.to_path_buf(), collect_files(path)?))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  fx parse <file.fx>");
    eprintln!("  fx symbols [root]");
    eprintln!("  fx check [--json] [root-or-file]");
    eprintln!("  fx compile --target js [--out-dir dir] [root-or-file]");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckOptions {
    path: PathBuf,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompileCliOptions {
    path: PathBuf,
    target: CompileTarget,
    out_dir: Option<PathBuf>,
}

impl CompileCliOptions {
    fn parse(args: Vec<String>) -> Result<Self> {
        let mut target = None;
        let mut out_dir = None;
        let mut path = None;
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--target" => {
                    let value = iter
                        .next()
                        .with_context(|| "`compile --target` requires a target")?;
                    target = Some(parse_compile_target(&value)?);
                }
                "--out-dir" => {
                    let value = iter
                        .next()
                        .with_context(|| "`compile --out-dir` requires a directory")?;
                    out_dir = Some(PathBuf::from(value));
                }
                _ if path.is_none() => path = Some(PathBuf::from(arg)),
                _ => bail!("unexpected argument `{arg}`"),
            }
        }

        let Some(target) = target else {
            bail!("`compile` requires `--target js`");
        };

        Ok(Self {
            path: path.unwrap_or_else(|| PathBuf::from(".")),
            target,
            out_dir,
        })
    }
}

fn parse_compile_target(target: &str) -> Result<CompileTarget> {
    match target {
        "js" | "javascript" => Ok(CompileTarget::JavaScript),
        other => bail!("unknown compile target `{other}`"),
    }
}

impl CheckOptions {
    fn parse(args: Vec<String>) -> Result<Self> {
        let mut json = false;
        let mut path = None;

        for arg in args {
            match arg.as_str() {
                "--json" => json = true,
                _ if path.is_none() => path = Some(PathBuf::from(arg)),
                _ => bail!("unexpected argument `{arg}`"),
            }
        }

        Ok(Self {
            path: path.unwrap_or_else(|| PathBuf::from(".")),
            json,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: ExitCode,
}

fn check_output(options: &CheckOptions) -> Result<CommandOutput> {
    let (root, files) = collect_check_files(&options.path)?;
    let analysis = analyze_workspace(&root, files);
    render_check_output(&analysis, options.json)
}

fn compile_output(options: &CompileCliOptions) -> Result<CommandOutput> {
    let (root, files) = collect_check_files(&options.path)?;
    let analysis = analyze_workspace(&root, files);
    let session = match prepare_compile(
        analysis.clone(),
        CompileOptions {
            target: options.target,
        },
    ) {
        Ok(session) => session,
        Err(_) => return render_check_output(&analysis, false),
    };
    let program = match lower_to_core(&session) {
        Ok(program) => program,
        Err(diagnostics) => {
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: render_diagnostics(&session.analysis, &diagnostics),
                exit_code: ExitCode::FAILURE,
            });
        }
    };
    let modules = emit_javascript_modules(&program);

    if let Some(out_dir) = &options.out_dir {
        for module in &modules {
            let path = out_dir.join(&module.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create `{}`", parent.display()))?;
            }
            fs::write(&path, &module.source)
                .with_context(|| format!("failed to write `{}`", path.display()))?;
        }
        return Ok(CommandOutput {
            stdout: format!(
                "fx compile: wrote {} JavaScript module(s) to {}\n",
                modules.len(),
                out_dir.display()
            ),
            stderr: String::new(),
            exit_code: ExitCode::SUCCESS,
        });
    }

    let mut stdout = String::new();
    for module in modules {
        stdout.push_str(&format!("// file: {}\n", module.path));
        stdout.push_str(&module.source);
        if !stdout.ends_with('\n') {
            stdout.push('\n');
        }
    }
    Ok(CommandOutput {
        stdout,
        stderr: String::new(),
        exit_code: ExitCode::SUCCESS,
    })
}

fn render_check_output(analysis: &Analysis, json: bool) -> Result<CommandOutput> {
    let diagnostics = sorted_diagnostics(analysis);
    let exit_code = if diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    };

    if json {
        return Ok(CommandOutput {
            stdout: format!("{}\n", serde_json::to_string_pretty(&analysis.check)?),
            stderr: String::new(),
            exit_code,
        });
    }

    if diagnostics.is_empty() {
        return Ok(CommandOutput {
            stdout: "fx check: no diagnostics\n".to_string(),
            stderr: String::new(),
            exit_code,
        });
    }

    Ok(CommandOutput {
        stdout: String::new(),
        stderr: render_diagnostics(analysis, &diagnostics),
        exit_code,
    })
}

fn sorted_diagnostics(analysis: &Analysis) -> Vec<AnalysisDiagnostic> {
    let mut diagnostics = analysis
        .diagnostics_by_path()
        .into_values()
        .flatten()
        .collect::<Vec<_>>();
    diagnostics.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.span.start.cmp(&b.span.start))
            .then(a.message.cmp(&b.message))
    });
    diagnostics
}

fn render_diagnostics(analysis: &Analysis, diagnostics: &[AnalysisDiagnostic]) -> String {
    let sources = sources_by_path(analysis);
    let mut output = String::new();

    for diagnostic in diagnostics {
        let Some(source) = sources.get(&diagnostic.path) else {
            output.push_str(&format!(
                "{}:1:1: error: {}\n\n",
                diagnostic.path.display(),
                diagnostic.message
            ));
            continue;
        };
        output.push_str(&render_diagnostic(source, diagnostic));
        output.push('\n');
    }

    output
}

fn sources_by_path(analysis: &Analysis) -> BTreeMap<PathBuf, String> {
    analysis
        .graph
        .modules
        .values()
        .map(|module| (module.path.clone(), module.parsed.source.clone()))
        .collect()
}

fn render_diagnostic(source: &str, diagnostic: &AnalysisDiagnostic) -> String {
    let index = LineIndex::new(source);
    let location = index.location(diagnostic.span.start);
    let line = index.line(location.line);
    let underline = index.underline(diagnostic.span);

    format!(
        "{}:{}:{}: error: {}\n{}\n{}",
        diagnostic.path.display(),
        location.line + 1,
        location.column + 1,
        diagnostic.message,
        line,
        underline
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Location {
    line: usize,
    column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LineIndex {
    line_starts: Vec<usize>,
    source_len: usize,
    source: String,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (offset, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(offset + 1);
            }
        }
        Self {
            line_starts,
            source_len: source.len(),
            source: source.to_string(),
        }
    }

    fn location(&self, offset: usize) -> Location {
        let offset = offset.min(self.source_len);
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next.saturating_sub(1),
        };
        Location {
            line,
            column: offset.saturating_sub(self.line_starts[line]),
        }
    }

    fn line(&self, line: usize) -> &str {
        let start = self.line_starts[line];
        let end = self
            .line_starts
            .get(line + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.source_len);
        &self.source[start..end]
    }

    fn underline(&self, span: Span) -> String {
        let start = self.location(span.start);
        let line_end = self
            .line_starts
            .get(start.line + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.source_len);
        let end = span
            .end
            .max(span.start + 1)
            .min(line_end.max(span.start + 1));
        let width = end.saturating_sub(span.start).max(1);
        format!("{}{}", " ".repeat(start.column), "^".repeat(width))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fx_core::SourceFile;

    #[test]
    fn renders_single_line_diagnostic() {
        let analysis = analysis_with_source("bad.fx", "bad() : Int\n  \"nope\"\n");
        let rendered = render_diagnostics(&analysis, &sorted_diagnostics(&analysis));

        assert!(rendered.contains("bad.fx:2:3: error: expected `Int`, found `Text`"));
        assert!(rendered.contains("  \"nope\""));
        assert!(rendered.contains("  ^^^^^^"));
    }

    #[test]
    fn renders_multiline_diagnostic_first_line() {
        let diagnostic = AnalysisDiagnostic {
            module: "main".to_string(),
            path: PathBuf::from("main.fx"),
            span: Span::new(0, 14),
            message: "multiline".to_string(),
        };

        let rendered = render_diagnostic("first\nsecond\n", &diagnostic);

        assert!(rendered.contains("main.fx:1:1: error: multiline"));
        assert!(rendered.contains("first"));
        assert!(rendered.contains("^^^^^"));
    }

    #[test]
    fn renders_zero_width_diagnostic() {
        let diagnostic = AnalysisDiagnostic {
            module: "main".to_string(),
            path: PathBuf::from("main.fx"),
            span: Span::point(3),
            message: "point".to_string(),
        };

        let rendered = render_diagnostic("abc\n", &diagnostic);

        assert!(rendered.contains("main.fx:1:4: error: point"));
        assert!(rendered.contains("   ^"));
    }

    #[test]
    fn sorts_diagnostics_by_path_and_position() {
        let analysis = analyze_workspace(
            &PathBuf::from("."),
            vec![
                source("b.fx", "bad() : Int\n  \"nope\"\n"),
                source("a.fx", "bad() : Int\n  \"nope\"\n"),
            ],
        );

        let diagnostics = sorted_diagnostics(&analysis);

        assert_eq!(diagnostics[0].path, PathBuf::from("a.fx"));
        assert_eq!(diagnostics[1].path, PathBuf::from("b.fx"));
    }

    #[test]
    fn check_human_mode_reports_diagnostics_and_failure() {
        let analysis = analysis_with_source("bad.fx", "bad() : Int\n  \"nope\"\n");
        let output = render_check_output(&analysis, false).expect("output");

        assert_eq!(output.exit_code, ExitCode::FAILURE);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains("bad.fx:2:3: error"));
    }

    #[test]
    fn check_json_mode_reports_machine_readable_output_and_failure() {
        let analysis = analysis_with_source("bad.fx", "bad() : Int\n  \"nope\"\n");
        let output = render_check_output(&analysis, true).expect("output");

        assert_eq!(output.exit_code, ExitCode::FAILURE);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("\"diagnostics\""));
    }

    #[test]
    fn check_human_mode_reports_success() {
        let analysis = analysis_with_source("ok.fx", "ok() : Int\n  1\n");
        let output = render_check_output(&analysis, false).expect("output");

        assert_eq!(output.exit_code, ExitCode::SUCCESS);
        assert_eq!(output.stdout, "fx check: no diagnostics\n");
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn check_human_mode_accepts_phase_2_examples() {
        let output = check_output(&CheckOptions {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/phase-2"),
            json: false,
        })
        .expect("phase 2 output");

        assert_eq!(output.exit_code, ExitCode::SUCCESS);
        assert_eq!(output.stdout, "fx check: no diagnostics\n");
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn check_options_accept_json_before_path() {
        let options =
            CheckOptions::parse(vec!["--json".to_string(), "src".to_string()]).expect("options");

        assert!(options.json);
        assert_eq!(options.path, PathBuf::from("src"));
    }

    #[test]
    fn compile_options_require_target() {
        let error = CompileCliOptions::parse(vec!["src".to_string()]).expect_err("target error");

        assert!(error.to_string().contains("requires `--target js`"));
    }

    #[test]
    fn compile_options_reject_unknown_targets() {
        let error = CompileCliOptions::parse(vec![
            "--target".to_string(),
            "c".to_string(),
            "src".to_string(),
        ])
        .expect_err("target error");

        assert!(error.to_string().contains("unknown compile target `c`"));
    }

    #[test]
    fn compile_stdout_emits_javascript() {
        let output = compile_output(&CompileCliOptions {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/phase-2.5"),
            target: CompileTarget::JavaScript,
            out_dir: None,
        })
        .expect("compile output");

        assert_eq!(output.exit_code, ExitCode::SUCCESS);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("// file: main.mjs"));
        assert!(output.stdout.contains("export function __fx_user_main()"));
    }

    #[test]
    fn compile_rejects_diagnostics_before_emitting() {
        let dir = temp_dir("fx-cli-compile-bad");
        fs::write(dir.join("bad.fx"), "bad() : Int\n  missing\n").expect("write bad file");

        let output = compile_output(&CompileCliOptions {
            path: dir.clone(),
            target: CompileTarget::JavaScript,
            out_dir: Some(dir.join("out")),
        })
        .expect("compile output");

        let _ = fs::remove_dir_all(&dir);

        assert_eq!(output.exit_code, ExitCode::FAILURE);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains("unknown name `missing`"));
    }

    #[test]
    fn compile_reports_compiler_unsupported_diagnostics_before_emitting() {
        let dir = temp_dir("fx-cli-compile-unsupported");
        fs::write(
            dir.join("tuple.fx"),
            "type Pair = (Int, Text)\n\nvalue() : Int\n  1\n",
        )
        .expect("write unsupported file");

        let output = compile_output(&CompileCliOptions {
            path: dir.clone(),
            target: CompileTarget::JavaScript,
            out_dir: Some(dir.join("out")),
        })
        .expect("compile output");

        let _ = fs::remove_dir_all(&dir);

        assert_eq!(output.exit_code, ExitCode::FAILURE);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains("tuple.fx:1:13: error: Phase 2.5 compiler unsupported: only record and variant type declarations are executable in Phase 2.5"));
        assert!(output.stderr.contains("type Pair = (Int, Text)"));
        assert!(output.stderr.contains("^"));
    }

    #[test]
    fn compile_outputs_phase_3_effect_runtime() {
        let output = compile_output(&CompileCliOptions {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/phase-3"),
            target: CompileTarget::JavaScript,
            out_dir: None,
        })
        .expect("compile output");

        assert_eq!(output.exit_code, ExitCode::SUCCESS);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("__fx.perform"));
        assert!(output.stdout.contains("__fx.handle"));
    }

    #[test]
    fn compile_out_dir_outputs_executable_phase_2_5_modules() {
        let dir = temp_dir("fx-cli-compile-out");
        let out_dir = dir.join("out");
        let output = compile_output(&CompileCliOptions {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/phase-2.5"),
            target: CompileTarget::JavaScript,
            out_dir: Some(out_dir.clone()),
        })
        .expect("compile output");

        assert_eq!(output.exit_code, ExitCode::SUCCESS);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("wrote 3 JavaScript module"));
        assert!(out_dir.join("main.mjs").exists());

        let node = std::process::Command::new("node")
            .arg("-e")
            .arg(format!(
                "import('{}/main.mjs').then((m) => console.log(m.__fx_user_main()))",
                out_dir.display()
            ))
            .output()
            .expect("node runs compiled output");

        let _ = fs::remove_dir_all(&dir);

        assert!(
            node.status.success(),
            "node failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&node.stdout),
            String::from_utf8_lossy(&node.stderr)
        );
        assert_eq!(
            String::from_utf8(node.stdout).expect("utf8 stdout").trim(),
            "Hello, Grace!! phase 2.5"
        );
    }

    #[test]
    fn collect_check_files_accepts_single_file() {
        let dir = temp_dir("fx-cli-test");
        let file = dir.join("single.fx");
        fs::write(&file, "ok() : Int\n  1\n").expect("write temp file");

        let (root, files) = collect_check_files(&file).expect("collect file");

        assert_eq!(root, dir);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, file);
        assert_eq!(files[0].source, "ok() : Int\n  1\n");

        let _ = fs::remove_dir_all(&dir);
    }

    fn analysis_with_source(path: &str, source_text: &str) -> Analysis {
        analyze_workspace(&PathBuf::from("."), vec![source(path, source_text)])
    }

    fn source(path: &str, source_text: &str) -> SourceFile {
        SourceFile {
            path: PathBuf::from(path),
            source: source_text.to_string(),
        }
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
