use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use kagura_interp::RuntimeErrorKind;
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
    pub output: String,
}

pub fn discover(root: &Path) -> io::Result<Vec<TestCase>> {
    let mut cases = Vec::new();
    visit(root, &mut cases)?;
    cases.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(cases)
}

pub fn skip_reason(metadata: &Metadata) -> Option<&'static str> {
    const SUPPORTED_FEATURES: &[&str] = &[
        "arrow-function",
        "let",
        "const",
        "default-parameters",
        "rest-parameters",
        "Array.from",
        "Array.of",
        "Array.prototype.includes",
        "Array.prototype.map",
        "Array.prototype.filter",
        "Array.prototype.forEach",
        "Array.prototype.some",
        "Array.prototype.every",
        "Array.prototype.find",
        "Array.prototype.findIndex",
        "Array.prototype.fill",
        "Array.prototype.at",
        "Array.prototype.lastIndexOf",
        "Array.prototype.reduce",
        "Array.prototype.reduceRight",
        "Array.prototype.copyWithin",
        "Array.prototype.flat",
        "Array.prototype.flatMap",
        "Array.prototype.reverse",
        "Array.prototype.concat",
        "Object.assign",
        "Object.keys",
        "Object.values",
        "Object.entries",
        "Object.create",
        "Object.defineProperty",
        "Object.freeze",
        "Object.seal",
        "Object.hasOwn",
        "String.prototype.includes",
        "String.prototype.startsWith",
        "String.prototype.endsWith",
        "String.prototype.repeat",
        "String.prototype.replace",
        "String.prototype.replaceAll",
        "String.prototype.trimStart",
        "String.prototype.trimEnd",
        "String.prototype.split",
        "String.prototype.padStart",
        "String.prototype.padEnd",
        "String.prototype.codePointAt",
        "String.prototype.substr",
        "String.prototype.at",
        "String.prototype.concat",
        "String.prototype.localeCompare",
        "Number.prototype.toFixed",
        "Number.prototype.toExponential",
        "Number.prototype.toPrecision",
        "Number.isNaN",
        "Number.isFinite",
        "Number.isInteger",
        "Number.isSafeInteger",
        "Number.parseInt",
        "Number.parseFloat",
        "String.fromCharCode",
        "String.fromCodePoint",
    ];
    if metadata
        .flags
        .iter()
        .any(|flag| matches!(flag.as_str(), "module" | "async" | "raw"))
    {
        return Some("unsupported test262 flag");
    }
    if metadata
        .features
        .iter()
        .any(|feature| !SUPPORTED_FEATURES.contains(&feature.as_str()))
    {
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
            output: String::new(),
        };
    }
    if let Some(assertion) = unsupported_assertion(&case.source) {
        return CaseReport {
            path: case.path.clone(),
            outcome: Outcome::Skip,
            message: format!("unsupported assertion helper: {assertion}"),
            output: String::new(),
        };
    }

    let source = match source_with_includes(case) {
        Ok(source) => source,
        Err(message) => {
            return CaseReport {
                path: case.path.clone(),
                outcome: Outcome::Fail,
                message,
                output: String::new(),
            };
        }
    };

    let mut host = Test262Host::default();
    let mut runtime = Runtime::new(4 << 20);
    runtime.set_step_limit(5_000);
    let result = runtime.run(&source, &mut host);
    let (outcome, message) = match (case.metadata.negative.as_ref(), result) {
        (None, Ok(_)) => (Outcome::Pass, String::new()),
        (Some(negative), Err(error)) => {
            if negative_matches(negative, &error) {
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
        output: host.output(),
    }
}

fn negative_matches(negative: &crate::metadata::Negative, error: &RunError) -> bool {
    match negative.phase.as_str() {
        "parse" | "early" => matches!(error, RunError::Parse(_) | RunError::Compile(_)),
        "runtime" => {
            let RunError::Runtime(error) = error else {
                return false;
            };
            match negative.kind.as_str() {
                "SyntaxError" => error.kind == RuntimeErrorKind::Syntax,
                "TypeError" => error.kind == RuntimeErrorKind::Type,
                "ReferenceError" => error.kind == RuntimeErrorKind::Reference,
                "RangeError" => error.kind == RuntimeErrorKind::Range,
                "Error" => error.kind == RuntimeErrorKind::Runtime,
                // Test262Error and host-thrown objects surface as Runtime;
                // they do not have a dedicated RuntimeErrorKind yet.
                _ => error.kind == RuntimeErrorKind::Runtime,
            }
        }
        _ => false,
    }
}

fn source_with_includes(case: &TestCase) -> Result<String, String> {
    let harness = harness_dir_for(&case.path);
    let mut source = String::new();
    for include in &case.metadata.includes {
        if matches!(include.as_str(), "assert.js" | "sta.js") {
            continue;
        }
        let Some(harness) = harness.as_ref() else {
            return Err(format!("cannot locate harness for include '{include}'"));
        };
        let path = harness.join(include);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read include '{}': {error}", path.display()))?;
        source.push_str(&text);
        source.push('\n');
    }
    if case.metadata.flags.iter().any(|flag| flag == "onlyStrict") {
        source.push_str("\"use strict\";\n");
    }
    source.push_str(&case.source);
    Ok(source)
}

fn harness_dir_for(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory.file_name().is_some_and(|name| name == "test") {
            let harness = directory.parent()?.join("harness");
            if harness.join("assert.js").is_file() {
                return Some(harness);
            }
        }
        current = directory.parent();
    }
    None
}

fn unsupported_assertion(source: &str) -> Option<&'static str> {
    const UNSUPPORTED: &[&str] = &["assert.throwsAsync"];
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
