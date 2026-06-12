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
        "main",
    );

    assert_eq!(output, "([Grace!]) outerctx:inner(body)");
}

#[test]
fn handled_raise_effect_executes() {
    let output = compile_and_run_main(
        "handled_raise_effect",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/main.fx",
            r#"export effect Raise
  raise(message: Text) : a

safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x

catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(_message, _resume) ->
      default

    return(value) ->
      value

export main() : Int
  with catch(0)
    safe-divide(10, 0)
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "0");
}

#[test]
fn resumptive_choose_effect_executes() {
    let output = compile_and_run_main(
        "resumptive_choose_effect",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/main.fx",
            r#"export effect Choose
  choose(left: a, right: a) : a

choose-first(action: () -> a with <Choose, ...e>) : a with <...e>
  handle action()
    choose(left, _right, resume) ->
      resume(left)

    return(value) ->
      value

export main() : Int
  with choose-first()
    choose(1, 2) + 10
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "11");
}

#[test]
fn phase_3_raise_fixture_executes_observable_value() {
    let output = compile_and_run_main(
        "phase_3_raise_fixture",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/effects.fx",
            include_str!("../../../examples/phase-3/effects.fx"),
        )],
        "effects",
        "value",
    );

    assert_eq!(output, "0");
}

#[test]
fn phase_3_resume_fixture_executes_observable_main() {
    let output = compile_and_run_main(
        "phase_3_resume_fixture",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/resume.fx",
            include_str!("../../../examples/phase-3/resume.fx"),
        )],
        "resume",
        "main",
    );

    assert_eq!(output, "11");
}

#[test]
fn phase_3_resume_fixture_runs_directly_under_node() {
    let output = compile_and_run_module_directly(
        "phase_3_resume_direct",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/resume.fx",
            include_str!("../../../examples/phase-3/resume.fx"),
        )],
        "resume",
    );

    assert_eq!(output, "11");
}

#[test]
fn phase_3_raise_fixture_runs_directly_under_node() {
    let output = compile_and_run_module_directly(
        "phase_3_raise_direct",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/effects.fx",
            include_str!("../../../examples/phase-3/effects.fx"),
        )],
        "effects",
    );

    assert_eq!(output, "0");
}

#[test]
fn direct_unit_main_does_not_print_undefined() {
    let output = compile_and_run_module_directly(
        "direct_unit_main",
        "smoke",
        vec![source(
            "smoke/main.fx",
            r#"export main() : Unit
  unit
"#,
        )],
        "main",
    );

    assert_eq!(output, "");
}

#[test]
fn effectful_expression_statements_execute_in_order() {
    let output = compile_and_run_main(
        "effectful_expression_statements",
        "examples/phase-3",
        vec![source(
            "examples/phase-3/main.fx",
            r#"export effect Emit
  emit(value: Text) : Unit

emit-twice(value: Text) : Unit with <Emit>
  emit(value ++ "1")
  emit(value ++ "2")
  unit

collect(action: () -> Unit with <Emit>) : Text
  first = ""

  handle action()
    emit(value, resume) ->
      resume(unit) ++ value

    return(_value) ->
      first

export main() : Text
  with collect()
    emit-twice("event")
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "event2event1");
}

#[test]
fn scoped_explicit_operation_executes() {
    let output = compile_and_run_main(
        "scoped_explicit_operation",
        "examples/phase-4",
        vec![source(
            "examples/phase-4/main.fx",
            r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export main() : Int
  with with-state(Counter, 41)
    get(Counter) + 1
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "42");
}

#[test]
fn scoped_within_elision_executes() {
    let output = compile_and_run_main(
        "scoped_within_elision",
        "examples/phase-4",
        vec![source(
            "examples/phase-4/main.fx",
            r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export main() : Int
  with with-state(Counter, 41)
    within Counter
      get() + 1
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "42");
}

#[test]
fn within_elides_scope_valued_function_arguments() {
    let output = compile_and_run_main(
        "within_elides_scope_function_arguments",
        "examples/phase-4",
        vec![source(
            "examples/phase-4/main.fx",
            r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export main() : Int
  within Counter
    with with-state(41)
      get() + 1
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "42");
}

#[test]
fn scoped_same_family_other_scope_remains_unhandled_at_runtime() {
    let error = compile_and_run_module_directly_error(
        "scoped_other_scope_unhandled",
        "examples/phase-4",
        vec![source(
            "examples/phase-4/main.fx",
            r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>
scope OtherCounter with <State Int>

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export main() : Int with <State OtherCounter Int>
  with with-state(Counter, 0)
    within Counter
      get()
      get(OtherCounter)
"#,
        )],
        "main",
    );

    assert!(error.contains("Unhandled fx operation main.get at main.OtherCounter"));
}

#[test]
fn scoped_polymorphic_helper_executes_with_concrete_scope() {
    let output = compile_and_run_main(
        "scoped_polymorphic_helper",
        "examples/phase-4",
        vec![source(
            "examples/phase-4/main.fx",
            r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

increment(cell: s) : Int with <State s Int>
  within cell
    n = get()
    put(n + 1)
    n

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export main() : Int
  with with-state(Counter, 41)
    increment(Counter) + 1
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "42");
}

#[test]
fn scoped_exported_scope_identity_executes_across_modules() {
    let output = compile_and_run_main(
        "scoped_exported_scope_identity",
        "smoke",
        vec![
            source(
                "smoke/support.fx",
                r#"export scoped effect State s a
  get() : a
  put(value: a) : Unit

export scope Counter with <State Int>

export with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value
"#,
            ),
            source(
                "smoke/main.fx",
                r#"import support

export main() : Int
  with with-state(Counter, 41)
    get(Counter) + 1
"#,
            ),
        ],
        "main",
        "main",
    );

    assert_eq!(output, "42");
}

#[test]
fn contextual_function_executes_with_given_provider() {
    let output = compile_and_run_main(
        "contextual_function_given_provider",
        "smoke",
        vec![source(
            "smoke/main.fx",
            r#"type Clock = {
  now: Text
}

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ " " ++ message

export main() : Text
  given { clock = { now: "10:00" } }
    stamp("started")
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "10:00 started");
}

#[test]
fn context_polymorphic_with_clock_executes() {
    let output = compile_and_run_main(
        "context_polymorphic_with_clock",
        "smoke",
        vec![source(
            "smoke/main.fx",
            r#"type Clock = {
  now: Text
}

with-clock(clock: Clock, action: () -> a given { clock: Clock, ...c }) : a given { ...c }
  given { clock = clock }
    action()

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ " " ++ message

export main() : Text
  with with-clock({ now: "10:00" })
    stamp("started")
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "10:00 started");
}

#[test]
fn handler_clause_body_uses_contextual_value() {
    let output = compile_and_run_main(
        "handler_clause_contextual_value",
        "smoke",
        vec![source(
            "smoke/main.fx",
            r#"type Clock = {
  now: Text
}

effect Emit
  emit(value: Text) : Unit

collect(action: () -> Unit with <Emit> given { clock: Clock }) : Text given { clock: Clock }
  handle action()
    emit(value, resume) ->
      resume(unit) ++ clock.now ++ ":" ++ value

    return(_value) ->
      ""

export main() : Text
  given { clock = { now: "10:00" } }
    with collect()
      emit("event")
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "10:00:event");
}

#[test]
fn resumed_continuations_share_operation_site_context() {
    let output = compile_and_run_main(
        "resumed_continuations_share_context",
        "smoke",
        vec![source(
            "smoke/main.fx",
            r#"type Clock = {
  now: Text
}

effect Choose
  choose(left: Text, right: Text) : Text

choose-both(action: () -> Text with <Choose> given { clock: Clock }) : Text given { clock: Clock }
  handle action()
    choose(left, right, resume) ->
      resume(left) ++ "/" ++ resume(right)

    return(value) ->
      value

export main() : Text
  given { clock = { now: "10:00" } }
    with choose-both()
      clock.now ++ choose("", "!")
"#,
        )],
        "main",
        "main",
    );

    assert_eq!(output, "10:00/10:00!");
}

fn compile_and_run_main(
    prefix: &str,
    root: &str,
    files: Vec<SourceFile>,
    module: &str,
    function: &str,
) -> String {
    let analysis = analyze_workspace(&PathBuf::from(root), files);
    let session = prepare_compile(analysis, CompileOptions::default()).expect("compile session");
    let program = lower_to_core(&session).expect("core program");
    let dir = temp_dir(prefix);
    write_modules(&dir, emit_javascript_modules(&program));

    let output = Command::new("node")
        .arg("-e")
        .arg(format!(
            "import('{}/{}.mjs').then((m) => console.log(m.{}()))",
            dir.display(),
            module,
            fx_function_name(function)
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

fn compile_and_run_module_directly_error(
    prefix: &str,
    root: &str,
    files: Vec<SourceFile>,
    module: &str,
) -> String {
    let analysis = analyze_workspace(&PathBuf::from(root), files);
    let session = prepare_compile(analysis, CompileOptions::default()).expect("compile session");
    let program = lower_to_core(&session).expect("core program");
    let dir = temp_dir(prefix);
    write_modules(&dir, emit_javascript_modules(&program));

    let output = Command::new("node")
        .arg(dir.join(format!("{module}.mjs")))
        .output()
        .expect("node executes generated JavaScript directly");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !output.status.success(),
        "node succeeded unexpectedly\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stderr)
        .expect("node stderr is utf8")
        .trim()
        .to_string()
}

fn compile_and_run_module_directly(
    prefix: &str,
    root: &str,
    files: Vec<SourceFile>,
    module: &str,
) -> String {
    let analysis = analyze_workspace(&PathBuf::from(root), files);
    let session = prepare_compile(analysis, CompileOptions::default()).expect("compile session");
    let program = lower_to_core(&session).expect("core program");
    let dir = temp_dir(prefix);
    write_modules(&dir, emit_javascript_modules(&program));

    let output = Command::new("node")
        .arg(dir.join(format!("{module}.mjs")))
        .output()
        .expect("node executes generated JavaScript directly");

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

fn fx_function_name(name: &str) -> String {
    let mut output = String::from("__fx_user_");
    for (index, ch) in name.chars().enumerate() {
        if (index == 0 && (ch == '_' || ch == '$' || ch.is_ascii_alphabetic()))
            || (index > 0 && (ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        {
            output.push(ch);
        } else {
            output.push('_');
            output.push_str(&(ch as u32).to_string());
            output.push('_');
        }
    }
    output
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
