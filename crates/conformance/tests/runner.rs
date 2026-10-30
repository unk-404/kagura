use kagura_conformance::{
    Metadata, Negative, Outcome, TestCase, parse_metadata, run_case, skip_reason,
};

#[test]
fn parses_test262_frontmatter() {
    let metadata = parse_metadata(
        "/*---\nflags: [noStrict]\nfeatures: [Symbol.iterator, Array.prototype.map]\nincludes: [assert.js]\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\n",
    );
    assert_eq!(metadata.flags, ["noStrict"]);
    assert_eq!(
        metadata.features,
        ["Symbol.iterator", "Array.prototype.map"]
    );
    assert_eq!(metadata.includes, ["assert.js"]);
    assert_eq!(
        metadata.negative,
        Some(Negative {
            phase: "runtime".into(),
            kind: "TypeError".into(),
        })
    );
}

#[test]
fn skips_unsupported_harness_and_flags() {
    let mut metadata = Metadata::default();
    assert_eq!(skip_reason(&metadata), None);
    metadata.flags.push("module".into());
    assert!(skip_reason(&metadata).is_some());

    let metadata = Metadata {
        includes: vec!["helper.js".into()],
        ..Metadata::default()
    };
    assert!(skip_reason(&metadata).is_some());

    let metadata = Metadata {
        includes: vec!["assert.js".into(), "sta.js".into()],
        ..Metadata::default()
    };
    assert_eq!(skip_reason(&metadata), None);
}

#[test]
fn runs_test262_assertions() {
    let pass = TestCase {
        path: "assert-pass.js".into(),
        source: "assert.sameValue(1, 1);".into(),
        metadata: Metadata {
            includes: vec!["assert.js".into(), "sta.js".into()],
            ..Metadata::default()
        },
    };
    assert_eq!(run_case(&pass).outcome, Outcome::Pass);

    let fail = TestCase {
        path: "assert-fail.js".into(),
        source: "assert.sameValue(1, 2);".into(),
        metadata: Metadata {
            includes: vec!["assert.js".into(), "sta.js".into()],
            ..Metadata::default()
        },
    };
    assert_eq!(run_case(&fail).outcome, Outcome::Fail);
}

#[test]
fn runs_a_simple_case() {
    let case = TestCase {
        path: "simple.js".into(),
        source: "1 + 1;".into(),
        metadata: Metadata::default(),
    };
    assert_eq!(run_case(&case).outcome, Outcome::Pass);
}

#[test]
fn classifies_runtime_failure() {
    let case = TestCase {
        path: "bad.js".into(),
        source: "missingName;".into(),
        metadata: Metadata::default(),
    };
    assert_eq!(run_case(&case).outcome, Outcome::Fail);
}
