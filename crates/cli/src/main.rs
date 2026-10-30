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
        } => run_file(file, print_ast, print_bytecode, trace),
        Command::Build { file, output } => build_file(file, output),
        Command::RunBytecode { file, trace } => run_bytecode(file, trace),
        Command::Typescript { path } => run_typescript(path),
        Command::Test262 {
            path,
            limit,
            fail_on_error,
            verbose,
            min_pass,
        } => run_test262(path, limit, fail_on_error, verbose, min_pass),
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

fn run_file(file: PathBuf, print_ast: bool, print_bytecode: bool, trace: bool) -> i32 {
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

    let mut runtime = Runtime::default();
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

fn run_test262(
    path: PathBuf,
    limit: Option<usize>,
    fail_on_error: bool,
    verbose: bool,
    min_pass: Option<usize>,
) -> i32 {
    let worker = std::thread::Builder::new()
        .name("kagura-test262".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run_test262_inner(path, limit, fail_on_error, verbose, min_pass));
    match worker {
        Ok(worker) => worker.join().unwrap_or(2),
        Err(error) => {
            eprintln!("kagura: cannot start test262 worker: {error}");
            2
        }
    }
}

fn run_test262_inner(
    path: PathBuf,
    limit: Option<usize>,
    fail_on_error: bool,
    verbose: bool,
    min_pass: Option<usize>,
) -> i32 {
    let cases = match discover_test262(&path) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("kagura: cannot read {}: {error}", path.display());
            return 2;
        }
    };
    let selected = limit.map_or(cases.len(), |limit| limit.min(cases.len()));
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();
    let mut skip_reasons: BTreeMap<String, usize> = BTreeMap::new();
    for case in cases.iter().take(selected) {
        if verbose {
            eprintln!("{}", case.path.display());
        }
        let report = run_test262_case(case);
        match report.outcome {
            Outcome::Pass => passed += 1,
            Outcome::Fail => {
                failed += 1;
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
    println!("test262: {selected} selected, {passed} passed, {failed} failed, {skipped} skipped");
    for (reason, count) in skip_reasons.iter().take(10) {
        println!("skip {count}: {reason}");
    }
    for failure in &failures {
        println!("{failure}");
    }
    if failed > 0 && fail_on_error {
        return 1;
    }
    if min_pass.is_some_and(|minimum| passed < minimum) {
        return 1;
    }
    0
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
