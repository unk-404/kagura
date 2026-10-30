use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runner::TestCase;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct NodeResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_case(case: &TestCase, harness: &Path) -> io::Result<NodeResult> {
    let sta = std::fs::read_to_string(harness.join("sta.js"))?;
    let assert = std::fs::read_to_string(harness.join("assert.js"))?;
    let mut includes = String::new();
    for include in &case.metadata.includes {
        if matches!(include.as_str(), "assert.js" | "sta.js") {
            continue;
        }
        includes.push_str(&std::fs::read_to_string(harness.join(include))?);
        includes.push('\n');
    }
    let directory = std::env::temp_dir().join(format!(
        "kagura-test262-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("case.js");
    let source = format!("{sta}\n{assert}\n{includes}\n{}\n", case.source);
    std::fs::write(&path, source)?;
    let output = Command::new("node")
        .arg("--no-warnings")
        .arg(&path)
        .output();
    let _ = std::fs::remove_dir_all(&directory);
    let output = output?;
    Ok(NodeResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
