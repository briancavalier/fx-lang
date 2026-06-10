use fx_compiler::{
    CompileOptions, JavaScriptModule, emit_javascript_modules, lower_to_core, prepare_compile,
};
use fx_core::{SourceFile, analyze_workspace};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn phase_2_5_fixture_executes_observable_main() {
    let output = compile_and_run_main(
        "phase_2_5_fixture",
        "examples/phase-2.5",
        vec![
            source(
                "examples/phase-2.5/domain.fx",
                include_str!("../../../examples/phase-2.5/domain.fx"),
            ),
            source(
                "examples/phase-2.5/functions.fx",
                include_str!("../../../examples/phase-2.5/functions.fx"),
            ),
            source(
                "examples/phase-2.5/main.fx",
                include_str!("../../../examples/phase-2.5/main.fx"),
            ),
        ],
        "main",
    );

    assert_eq!(output, "Hello, Grace!! phase 2.5");
}

#[test]
fn records_updates_variants_matches_and_lists_execute() {
    let output = compile_and_run_main(
        "records_variants_lists",
        "smoke",
        vec![source(
            "smoke/main.fx",
            r#"
type Person = {
  name: Text,
  done: Bool
}

type Lookup = < Found(Person), Missing() >

find(wanted: Text, people: List Person) : Lookup
  match people
    [] ->
      Missing()

    [person, ...rest] ->
      if person.name == wanted then Found(person) else find(wanted, rest)

render(lookup: Lookup) : Text
  match lookup
    Found(person) ->
      person.name

    other ->
      "fallback"

export main() : Text
  first = { name: "Grace", done: false }

  updated = { ...first, done: true }

  render(find("Grace", [updated, { name: "Ada", done: false }]))
"#,
        )],
        "main",
    );

    assert_eq!(output, "Grace");
}

#[test]
fn functions_lambdas_recursion_and_imports_execute() {
    let output = compile_and_run_main(
        "functions_lambdas_imports",
        "smoke",
        vec![
            source(
                "smoke/support.fx",
                r#"
export apply-twice(f: Text -> Text, value: Text) : Text
  f(f(value))

export make-suffix(suffix: Text) : Text -> Text
  value -> value ++ suffix

export countdown(n: Int) : Text
  if n == 0 then "done" else countdown(n - 1)
"#,
            ),
            source(
                "smoke/main.fx",
                r#"
import support

export main() : Text
  bang = make-suffix("!")

  apply-twice(bang, countdown(2))
"#,
            ),
        ],
        "main",
    );

    assert_eq!(output, "done!!");
}

#[test]
fn phase_2_6_pipeline_and_with_fixture_executes() {
    let output = compile_and_run_main(
        "phase_2_6_fixture",
        "examples/phase-2.6",
        vec![
            source(
                "examples/phase-2.6/support.fx",
                include_str!("../../../examples/phase-2.6/support.fx"),
            ),
            source(
                "examples/phase-2.6/main.fx",
                include_str!("../../../examples/phase-2.6/main.fx"),
            ),
        ],
        "main",
    );

    assert_eq!(output, "([Grace!]) outerctx:inner(body)");
}

fn compile_and_run_main(prefix: &str, root: &str, files: Vec<SourceFile>, module: &str) -> String {
    let analysis = analyze_workspace(&PathBuf::from(root), files);
    let session = prepare_compile(analysis, CompileOptions::default()).expect("compile session");
    let program = lower_to_core(&session).expect("core program");
    let dir = temp_dir(prefix);
    write_modules(&dir, emit_javascript_modules(&program));

    let output = Command::new("node")
        .arg("-e")
        .arg(format!(
            "import('{}/{}.mjs').then((m) => console.log(m.__fx_user_main()))",
            dir.display(),
            module
        ))
        .output()
        .expect("node executes generated JavaScript");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "node failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .expect("node stdout is utf8")
        .trim()
        .to_string()
}

fn source(path: &str, source: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from(path),
        source: source.to_string(),
    }
}

fn temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fx-generated-smoke-{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_modules(dir: &Path, modules: Vec<JavaScriptModule>) {
    for module in modules {
        let path = dir.join(module.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create module dir");
        }
        fs::write(path, module.source).expect("write module");
    }
}
