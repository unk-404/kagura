use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use kagura_bytecode::compile;
use kagura_diagnostics::{Diagnostic, render};
use kagura_interp::Host;
use kagura_parser::parse;
use kagura_runtime::Runtime;

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
}

struct StdoutHost;

impl Host for StdoutHost {
    fn print(&mut self, text: &str) {
        println!("{text}");
    }
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
    };
    std::process::exit(code);
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
    let mut host = StdoutHost;
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
    let mut host = StdoutHost;
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

#[cfg(feature = "debug")]
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::DEBUG)
        .try_init();
}

#[cfg(not(feature = "debug"))]
fn init_tracing() {}
