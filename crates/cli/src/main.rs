use std::path::PathBuf;

use clap::{Parser, Subcommand};
use kagura_bytecode::compile;
use kagura_diagnostics::{Diagnostic, render};
use kagura_interp::Host;
use kagura_parser::parse;
use kagura_runtime::{RunError, Runtime};

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
    };
    std::process::exit(code);
}

fn run_file(file: PathBuf, print_ast: bool, print_bytecode: bool, trace: bool) -> i32 {
    let source = match std::fs::read_to_string(&file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("kagura: cannot read {}: {error}", file.display());
            return 2;
        }
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
        Err(error) => match error {
            RunError::Parse(error) => report(
                &source,
                &path,
                &Diagnostic::syntax(error.message, error.span),
            ),
            RunError::Compile(error) => report(
                &source,
                &path,
                &Diagnostic::syntax(error.message, error.span),
            ),
            RunError::Runtime(error) => report(&source, &path, &error.diagnostic()),
        },
    }
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
