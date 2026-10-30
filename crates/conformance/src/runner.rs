use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use kagura_runtime::{RunError, Runtime};

use crate::host::Test262Host;
use crate::metadata::{Metadata, parse_metadata};

#[derive(Clone, Debug)]
pub struct TestCase {
    pub path: PathBuf,
    pub source: String,
    pub metadata: Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

#[derive(Clone, Debug)]
pub struct CaseReport {
    pub path: PathBuf,
    pub outcome: Outcome,
    pub message: String,
}

pub fn discover(root: &Path) -> io::Result<Vec<TestCase>> {
    let mut cases = Vec::new();
    visit(root, &mut cases)?;
    cases.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(cases)
}

pub fn skip_reason(metadata: &Metadata) -> Option<&'static str> {
    if metadata
        .flags
        .iter()
        .any(|flag| matches!(flag.as_str(), "module" | "async" | "onlyStrict" | "raw"))
    {
        return Some("unsupported test262 flag");
    }
    if !metadata.includes.is_empty()
        && metadata
            .includes
            .iter()
            .any(|include| !matches!(include.as_str(), "assert.js" | "sta.js"))
    {
        return Some("test262 includes beyond assert.js/sta.js are not supported yet");
    }
    if !metadata.features.is_empty() {
        return Some("test262 features not supported yet");
    }
    None
}

pub fn run_case(case: &TestCase) -> CaseReport {
    if let Some(reason) = skip_reason(&case.metadata) {
        return CaseReport {
            path: case.path.clone(),
            outcome: Outcome::Skip,
            message: reason.to_owned(),
        };
    }
    if let Some(assertion) = unsupported_assertion(&case.source) {
        return CaseReport {
            path: case.path.clone(),
            outcome: Outcome::Skip,
            message: format!("unsupported assertion helper: {assertion}"),
        };
    }

    let mut host = Test262Host::default();
    let mut runtime = Runtime::default();
    runtime.set_step_limit(5_000);
    let result = runtime.run(&case.source, &mut host);
    let (outcome, message) = match (case.metadata.negative.as_ref(), result) {
        (None, Ok(_)) => (Outcome::Pass, String::new()),
        (Some(negative), Err(error)) => {
            if phase_matches(&negative.phase, &error) {
                (
                    Outcome::Pass,
                    format!("expected {}:{}: {error}", negative.phase, negative.kind),
                )
            } else {
                (
                    Outcome::Fail,
                    format!(
                        "expected {}:{} but got a different error phase: {error}",
                        negative.phase, negative.kind
                    ),
                )
            }
        }
        (Some(_), Ok(_)) => (Outcome::Fail, "expected an error".into()),
        (None, Err(error)) => (Outcome::Fail, error.to_string()),
    };
    CaseReport {
        path: case.path.clone(),
        outcome,
        message,
    }
}

fn phase_matches(phase: &str, error: &RunError) -> bool {
    match phase {
        "parse" | "early" => matches!(error, RunError::Parse(_) | RunError::Compile(_)),
        "runtime" => matches!(error, RunError::Runtime(_)),
        _ => false,
    }
}

fn unsupported_assertion(source: &str) -> Option<&'static str> {
    const UNSUPPORTED: &[&str] = &[
        "assert.throws",
        "assert.throwsAsync",
        "assert.compareArray",
        "assert.deepEqual",
    ];
    UNSUPPORTED
        .iter()
        .copied()
        .find(|pattern| source.contains(pattern))
}

fn visit(root: &Path, cases: &mut Vec<TestCase>) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "harness") {
                continue;
            }
            visit(&path, cases)?;
        } else if path.extension().is_some_and(|extension| extension == "js") {
            let source = fs::read_to_string(&path)?;
            let metadata = parse_metadata(&source);
            cases.push(TestCase {
                path,
                source,
                metadata,
            });
        }
    }
    Ok(())
}
