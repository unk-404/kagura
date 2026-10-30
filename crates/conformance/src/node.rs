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
    let mut harness_source = String::new();
    harness_source.push_str(&std::fs::read_to_string(harness.join("sta.js"))?);
    harness_source.push('\n');
    harness_source.push_str(&std::fs::read_to_string(harness.join("assert.js"))?);
    harness_source.push('\n');
    for include in &case.metadata.includes {
        if matches!(include.as_str(), "assert.js" | "sta.js") {
            continue;
        }
        harness_source.push_str(&std::fs::read_to_string(harness.join(include))?);
        harness_source.push('\n');
    }

    let directory = std::env::temp_dir().join(format!(
        "kagura-test262-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory)?;
    let harness_path = directory.join("harness.js");
    let case_path = directory.join("case.js");
    let runner_path = directory.join("runner.js");
    std::fs::write(&harness_path, harness_source)?;
    let strict = if case.metadata.flags.iter().any(|flag| flag == "onlyStrict") {
        "\"use strict\";\n"
    } else {
        ""
    };
    std::fs::write(&case_path, format!("{strict}{}\n", case.source))?;
    std::fs::write(
        &runner_path,
        r#"const fs = require('fs');
const vm = require('vm');
const context = vm.createContext({
  console,
  print: (...args) => console.log(...args),
});
vm.runInContext(fs.readFileSync(process.argv[2], 'utf8'), context, { filename: process.argv[2] });
vm.runInContext(fs.readFileSync(process.argv[3], 'utf8'), context, { filename: process.argv[3] });
"#,
    )?;
    let output = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner_path)
        .arg(&harness_path)
        .arg(&case_path)
        .output();
    let _ = std::fs::remove_dir_all(&directory);
    let output = output?;
    Ok(NodeResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
