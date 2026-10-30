use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use kagura_bindgen::{Bindings, HostCall, HostError};
use kagura_bytecode::compile;
use kagura_conformance::typescript;
use kagura_conformance::{Outcome, discover as discover_test262, run_case as run_test262_case};
use kagura_diagnostics::{Diagnostic, render};
use kagura_parser::parse;
use kagura_runtime::Runtime;
use kagura_value::Value;

#[derive(Debug, Parser)]
#[command(
    name = "kagura",
    version,
    about = "Kagura JavaScript/TypeScript engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        file: PathBuf,
        #[arg(long)]
        print_ast: bool,
        #[arg(long)]
        print_bytecode: bool,
        #[arg(long)]
        trace: bool,
        #[arg(long, default_value_t = 1 << 20)]
        heap_size: usize,
    },
    Build {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    RunBytecode {
        file: PathBuf,
        #[arg(long)]
        trace: bool,
    },
    Typescript {
        path: PathBuf,
    },
    Test262 {
        path: PathBuf,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        fail_on_error: bool,
        #[arg(long)]
        verbose: bool,
        #[arg(long)]
        min_pass: Option<usize>,
        #[arg(long)]
        node: bool,
        #[arg(long)]
        node_all: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        harness: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Run {
            file,
            print_ast,
            print_bytecode,
            trace,
            heap_size,
        } => run_file(file, print_ast, print_bytecode, trace, heap_size),
        Command::Build { file, output } => build_file(file, output),
        Command::RunBytecode { file, trace } => run_bytecode(file, trace),
        Command::Typescript { path } => run_typescript(path),
        Command::Test262 {
            path,
            limit,
            fail_on_error,
            verbose,
            min_pass,
            node,
            node_all,
            json,
            filter,
            harness,
        } => run_test262(
            path,
            limit,
            fail_on_error,
            verbose,
            min_pass,
            node,
            node_all,
            json,
            filter,
            harness,
        ),
    };
    std::process::exit(code);
}

fn stdout_bindings() -> Bindings {
    let mut bindings = Bindings::new();
    bindings.bind("console.log", stdout_print);
    bindings.bind("print", stdout_print);
    bindings
}

fn stdout_print(call: &mut HostCall<'_>) -> Result<Value, HostError> {
    let text = call
        .args()
        .iter()
        .map(|value| call.heap().to_js_string(*value))
        .collect::<Vec<_>>()
        .join(" ");
    println!("{text}");
    Ok(Value::undefined())
}

fn run_file(
    file: PathBuf,
    print_ast: bool,
    print_bytecode: bool,
    trace: bool,
    heap_size: usize,
) -> i32 {
    let source = match std::fs::read_to_string(&file) {
        Ok(source) => source,
        Err(error) => return read_error(&file, &error),
    };
    let path = file.display().to_string();

    if print_ast || print_bytecode {
        let program = match parse(&source) {
            Ok(program) => program,
            Err(error) => {
                return report(
                    &source,
                    &path,
                    &Diagnostic::syntax(error.message, error.span),
                );
            }
        };
        if print_ast {
            println!("{program:#?}");
        }
        if print_bytecode {
            match compile(&program) {
                Ok(module) => println!("{module:#?}"),
                Err(error) => {
                    return report(
                        &source,
                        &path,
                        &Diagnostic::syntax(error.message, error.span),
                    );
                }
            }
        }
    }

    if trace {
        init_tracing();
    }

    let mut runtime = Runtime::new(heap_size);
    let mut host = stdout_bindings();
    match runtime.run(&source, &mut host) {
        Ok(_) => 0,
        Err(error) => report(&source, &path, &error.diagnostic()),
    }
}

fn build_file(file: PathBuf, output: PathBuf) -> i32 {
    let source = match std::fs::read_to_string(&file) {
        Ok(source) => source,
        Err(error) => return read_error(&file, &error),
    };
    let path = file.display().to_string();
    let program = match parse(&source) {
        Ok(program) => program,
        Err(error) => {
            return report(
                &source,
                &path,
                &Diagnostic::syntax(error.message, error.span),
            );
        }
    };
    let module = match compile(&program) {
        Ok(module) => module,
        Err(error) => {
            return report(
                &source,
                &path,
                &Diagnostic::syntax(error.message, error.span),
            );
        }
    };
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("kagura: cannot create {}: {error}", parent.display());
        return 2;
    }
    if let Err(error) = std::fs::write(&output, module.encode()) {
        eprintln!("kagura: cannot write {}: {error}", output.display());
        return 2;
    }
    0
}

fn run_bytecode(file: PathBuf, trace: bool) -> i32 {
    let bytes = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        Err(error) => return read_error(&file, &error),
    };
    if trace {
        init_tracing();
    }
    let mut runtime = Runtime::default();
    let mut host = stdout_bindings();
    match runtime.run_bytecode(&bytes, &mut host) {
        Ok(_) => 0,
        Err(error) => {
            eprintln!("{}: {error}", file.display());
            1
        }
    }
}

fn read_error(path: &Path, error: &std::io::Error) -> i32 {
    eprintln!("kagura: cannot read {}: {error}", path.display());
    2
}

fn report(source: &str, path: &str, diagnostic: &Diagnostic) -> i32 {
    eprintln!("{}", render(source, path, diagnostic));
    1
}

#[allow(clippy::too_many_arguments)]
fn run_test262(
    path: PathBuf,
    limit: Option<usize>,
    fail_on_error: bool,
    verbose: bool,
    min_pass: Option<usize>,
    node: bool,
    node_all: bool,
    json: bool,
    filter: Option<String>,
    harness: Option<PathBuf>,
) -> i32 {
    let worker = std::thread::Builder::new()
        .name("kagura-test262".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            run_test262_inner(
                path,
                limit,
                fail_on_error,
                verbose,
                min_pass,
                node,
                node_all,
                json,
                filter,
                harness,
            )
        });
    match worker {
        Ok(worker) => worker.join().unwrap_or(2),
        Err(error) => {
            eprintln!("kagura: cannot start test262 worker: {error}");
            2
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_test262_inner(
    path: PathBuf,
    limit: Option<usize>,
    fail_on_error: bool,
    verbose: bool,
    min_pass: Option<usize>,
    node: bool,
    node_all: bool,
    json: bool,
    filter: Option<String>,
    harness: Option<PathBuf>,
) -> i32 {
    let oracle_enabled = node || node_all;
    let cases = match discover_test262(&path) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("kagura: cannot read {}: {error}", path.display());
            return 2;
        }
    };
    let cases = if let Some(filter) = filter {
        let filter = filter.replace('\\', "/");
        cases
            .into_iter()
            .filter(|case| {
                case.path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .contains(&filter)
            })
            .collect()
    } else {
        cases
    };
    let selected = limit.map_or(cases.len(), |limit| limit.min(cases.len()));
    let harness = if oracle_enabled {
        let harness = harness.or_else(|| find_harness(&path));
        let Some(harness) = harness else {
            eprintln!("kagura: cannot find test262 harness (assert.js/sta.js)");
            return 2;
        };
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("kagura: --node requires node on PATH");
            return 2;
        }
        Some(harness)
    } else {
        None
    };
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut oracle_checked = 0usize;
    let mut oracle_matched = 0usize;
    let mut oracle_mismatched = 0usize;
    let mut oracle_both_failed = 0usize;
    let mut oracle_kagura_fail_node_pass = 0usize;
    let mut oracle_kagura_pass_node_fail = 0usize;
    let mut oracle_failures = Vec::new();
    let mut failures = Vec::new();
    let mut failure_categories: BTreeMap<String, usize> = BTreeMap::new();
    let mut skip_reasons: BTreeMap<String, usize> = BTreeMap::new();
    for case in cases.iter().take(selected) {
        if verbose {
            eprintln!("{}", case.path.display());
        }
        let report = run_test262_case(case);
        if let Some(harness) = &harness
            && report.outcome != Outcome::Skip
            && (node_all || report.outcome == Outcome::Pass)
        {
            oracle_checked += 1;
            let oracle = match kagura_conformance::node::run_case(case, harness) {
                Ok(oracle) => oracle,
                Err(error) => {
                    eprintln!("kagura: node oracle failed: {error}");
                    return 2;
                }
            };
            let negative = case.metadata.negative.is_some();
            let matched = if negative {
                report.outcome == Outcome::Pass && !oracle.success
            } else if report.outcome == Outcome::Pass {
                oracle.success && oracle.stdout == report.output
            } else {
                false
            };
            if matched {
                oracle_matched += 1;
            } else if report.outcome == Outcome::Fail && !oracle.success {
                oracle_both_failed += 1;
            } else {
                oracle_mismatched += 1;
                if report.outcome == Outcome::Fail && oracle.success {
                    oracle_kagura_fail_node_pass += 1;
                } else {
                    oracle_kagura_pass_node_fail += 1;
                }
                if oracle_failures.len() < 25 {
                    oracle_failures.push(format!(
                        "{}: kagura={:?} node_success={} node_stdout={:?} node_stderr={:?}",
                        report.path.display(),
                        report.outcome,
                        oracle.success,
                        oracle.stdout,
                        oracle.stderr.lines().next().unwrap_or_default()
                    ));
                }
            }
        }
        match report.outcome {
            Outcome::Pass => passed += 1,
            Outcome::Fail => {
                failed += 1;
                *failure_categories
                    .entry(classify_test262_failure(&report.message))
                    .or_default() += 1;
                if failures.len() < 25 {
                    failures.push(format!("{}: {}", report.path.display(), report.message));
                }
            }
            Outcome::Skip => {
                skipped += 1;
                *skip_reasons.entry(report.message).or_default() += 1;
            }
        }
    }
    if json {
        print_test262_json(
            selected,
            passed,
            failed,
            skipped,
            &skip_reasons,
            &failure_categories,
            &failures,
            harness.is_some(),
            oracle_checked,
            oracle_matched,
            oracle_mismatched,
            oracle_both_failed,
            oracle_kagura_fail_node_pass,
            oracle_kagura_pass_node_fail,
            &oracle_failures,
        );
    } else {
        println!(
            "test262: {selected} selected, {} executed, {passed} passed, {failed} failed, {skipped} skipped",
            passed + failed
        );
        for (reason, count) in skip_reasons.iter().take(10) {
            println!("skip {count}: {reason}");
        }
        for (category, count) in failure_categories.iter().take(10) {
            println!("failure {count}: {category}");
        }
        for failure in &failures {
            println!("{failure}");
        }
        if harness.is_some() {
            println!(
                "oracle node: {oracle_checked} checked, {oracle_matched} matched, {oracle_mismatched} mismatched, {oracle_both_failed} both-failed, {oracle_kagura_fail_node_pass} kagura-fail/node-pass, {oracle_kagura_pass_node_fail} kagura-pass/node-fail"
            );
            for failure in &oracle_failures {
                println!("{failure}");
            }
        }
    }
    if failed > 0 && fail_on_error {
        return 1;
    }
    if oracle_mismatched > 0 && fail_on_error {
        return 1;
    }
    if min_pass.is_some_and(|minimum| passed < minimum) {
        return 1;
    }
    0
}

#[allow(clippy::too_many_arguments)]
fn print_test262_json(
    selected: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
    skip_reasons: &BTreeMap<String, usize>,
    failure_categories: &BTreeMap<String, usize>,
    failures: &[String],
    oracle_enabled: bool,
    oracle_checked: usize,
    oracle_matched: usize,
    oracle_mismatched: usize,
    oracle_both_failed: usize,
    oracle_kagura_fail_node_pass: usize,
    oracle_kagura_pass_node_fail: usize,
    oracle_failures: &[String],
) {
    let skip_reasons = skip_reasons
        .iter()
        .map(|(reason, count)| format!("\"{}\":{count}", json_escape(reason)))
        .collect::<Vec<_>>()
        .join(",");
    let failure_categories = failure_categories
        .iter()
        .map(|(category, count)| format!("\"{}\":{count}", json_escape(category)))
        .collect::<Vec<_>>()
        .join(",");
    let failures = failures
        .iter()
        .map(|failure| format!("\"{}\"", json_escape(failure)))
        .collect::<Vec<_>>()
        .join(",");
    let oracle_failures = oracle_failures
        .iter()
        .map(|failure| format!("\"{}\"", json_escape(failure)))
        .collect::<Vec<_>>()
        .join(",");
    let oracle = if oracle_enabled {
        format!(
            "{{\"checked\":{oracle_checked},\"matched\":{oracle_matched},\"mismatched\":{oracle_mismatched},\"both_failed\":{oracle_both_failed},\"kagura_fail_node_pass\":{oracle_kagura_fail_node_pass},\"kagura_pass_node_fail\":{oracle_kagura_pass_node_fail},\"mismatches\":[{oracle_failures}]}}"
        )
    } else {
        "null".to_owned()
    };
    println!(
        "{{\"selected\":{selected},\"executed\":{},\"passed\":{passed},\"failed\":{failed},\"skipped\":{skipped},\"skip_reasons\":{{{skip_reasons}}},\"failure_categories\":{{{failure_categories}}},\"failures\":[{failures}],\"oracle\":{oracle}}}",
        passed + failed
    );
}

fn classify_test262_failure(message: &str) -> String {
    if message.contains("execution step limit") {
        return "step limit".into();
    }
    if message.contains("unknown variable") {
        return "unknown variable".into();
    }
    if message.contains("is not defined") {
        return "undefined global".into();
    }
    if message.contains("is not a function") {
        return "not callable".into();
    }
    if message.contains("is not a constructor") {
        return "not a constructor".into();
    }
    if message.contains("is not supported") {
        return "unsupported builtin".into();
    }
    if message.contains("expected ';'")
        || message.contains("expected ')'")
        || message.contains("expected ']'")
        || message.contains("unexpected character")
        || message.contains("unexpected token")
        || message.contains("unterminated")
        || message.contains("invalid assignment")
    {
        let detail = message.lines().next().unwrap_or("parse error");
        return format!("parse: {detail}");
    }
    if message.starts_with("expected an error")
        || message.contains("expected ")
        || message.contains("but caught")
    {
        return "negative/assert mismatch".into();
    }
    if message.contains("not an object") {
        return "type error".into();
    }
    "other".into()
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn find_harness(root: &Path) -> Option<PathBuf> {
    let mut candidates = vec![root.join("harness")];
    if let Some(parent) = root.parent() {
        candidates.push(parent.join("harness"));
    }
    candidates.into_iter().find(|candidate| {
        candidate.join("assert.js").is_file() && candidate.join("sta.js").is_file()
    })
}

fn run_typescript(path: PathBuf) -> i32 {
    match typescript::run(&path) {
        Ok(report) => {
            print!("{report}");
            if report.failed == 0 { 0 } else { 1 }
        }
        Err(error) => {
            eprintln!("kagura: cannot read {}: {error}", path.display());
            2
        }
    }
}

#[cfg(feature = "debug")]
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::DEBUG)
        .try_init();
}

#[cfg(not(feature = "debug"))]
fn init_tracing() {}
