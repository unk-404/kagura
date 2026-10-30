use std::fs;
use std::path::PathBuf;

use kagura_conformance::typescript::{discover, parse_directives, run};

fn fixture_directory() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("kagura-typescript-{}-{unique}", std::process::id()));
    fs::create_dir_all(directory.join("nested")).unwrap();
    directory
}

#[test]
fn parses_typescript_directives() {
    let directives = parse_directives("// @target: es2020\n// @strict: true\nconst value = 1;\n");
    assert_eq!(directives.len(), 2);
    assert_eq!(directives[0].key, "target");
    assert_eq!(directives[0].value, "es2020");
    assert_eq!(directives[1].key, "strict");
}

#[test]
fn runs_typescript_syntax_cases() {
    let directory = fixture_directory();
    fs::write(
        directory.join("nested").join("valid.ts"),
        "// @target: es2020\nlet value = 1;\n",
    )
    .unwrap();
    fs::write(directory.join("broken.ts"), "let = ;\n").unwrap();
    fs::write(
        directory.join("multi.ts"),
        "// @Filename: a.ts\nlet value = 1;\n// @Filename: b.ts\nlet other = 2;\n",
    )
    .unwrap();

    let cases = discover(&directory).unwrap();
    assert_eq!(cases.len(), 3);
    assert!(
        cases
            .iter()
            .any(|case| { case.path.ends_with("valid.ts") && case.target() == Some("es2020") })
    );

    let report = run(&directory).unwrap();
    assert_eq!(report.total, 3);
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.unsupported, 1);
    assert_eq!(report.failures.len(), 1);

    fs::remove_dir_all(directory).unwrap();
}
