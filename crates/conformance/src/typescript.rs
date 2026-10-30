use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directive {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub path: PathBuf,
    pub source: String,
    pub directives: Vec<Directive>,
}

impl Case {
    pub fn is_multi_file(&self) -> bool {
        self.directives
            .iter()
            .any(|directive| directive.key == "Filename")
    }

    pub fn target(&self) -> Option<&str> {
        self.directives
            .iter()
            .find(|directive| directive.key == "target")
            .map(|directive| directive.value.as_str())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub unsupported: usize,
    pub failures: Vec<(PathBuf, String)>,
}

impl fmt::Display for Report {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "typescript syntax: {} cases, {} passed, {} failed, {} unsupported",
            self.total, self.passed, self.failed, self.unsupported
        )?;
        for (path, message) in &self.failures {
            writeln!(formatter, "{}: {message}", path.display())?;
        }
        Ok(())
    }
}

pub fn parse_directives(source: &str) -> Vec<Directive> {
    let mut directives = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.trim_start().strip_prefix("// @") else {
            continue;
        };
        let Some((key, value)) = rest.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        directives.push(Directive {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    directives
}

pub fn discover(root: &Path) -> io::Result<Vec<Case>> {
    let mut cases = Vec::new();
    visit(root, &mut cases)?;
    cases.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(cases)
}

fn visit(directory: &Path, cases: &mut Vec<Case>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            visit(&path, cases)?;
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        cases.push(Case {
            directives: parse_directives(&source),
            path,
            source,
        });
    }
    Ok(())
}

/// Runs the syntax phase of the TypeScript conformance corpus.
///
/// TypeScript diagnostic baselines require a type checker and are not consumed here.
pub fn run(root: &Path) -> io::Result<Report> {
    let cases = discover(root)?;
    let mut report = Report {
        total: cases.len(),
        ..Report::default()
    };
    for case in cases {
        if case.is_multi_file() {
            report.unsupported += 1;
            continue;
        }
        match kagura_parser::parse(&case.source) {
            Ok(_) => report.passed += 1,
            Err(error) => {
                report.failed += 1;
                report.failures.push((case.path, error.message.to_string()));
            }
        }
    }
    Ok(report)
}
