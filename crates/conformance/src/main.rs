use std::path::PathBuf;

use clap::Parser;
use kagura_conformance::{Outcome, discover, run_case};

#[derive(Debug, Parser)]
#[command(name = "kagura-conformance", about = "Run test262 cases")]
struct Args {
    #[arg(long)]
    test262: PathBuf,
    #[arg(long)]
    filter: Option<String>,
    #[arg(long)]
    list: bool,
}

fn main() {
    let args = Args::parse();
    if !args.test262.is_dir() {
        eprintln!(
            "kagura-conformance: not a directory: {}",
            args.test262.display()
        );
        std::process::exit(2);
    }
    let cases = match discover(&args.test262) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("kagura-conformance: {error}");
            std::process::exit(2);
        }
    };
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|case| {
            args.filter
                .as_ref()
                .is_none_or(|filter| case.path.to_string_lossy().contains(filter))
        })
        .collect();

    if args.list {
        for case in &cases {
            println!("{}", case.path.display());
        }
        return;
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    for case in &cases {
        let report = run_case(case);
        match report.outcome {
            Outcome::Pass => passed += 1,
            Outcome::Fail => {
                failed += 1;
                eprintln!("FAIL {}: {}", report.path.display(), report.message);
            }
            Outcome::Skip => skipped += 1,
        }
    }
    println!("test262: {passed} passed, {failed} failed, {skipped} skipped");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
