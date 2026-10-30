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
fn parses_cr_only_metadata() {
    let metadata = parse_metadata("/*---\rflags: [noStrict]\rfeatures: [let]\r---*/\r");
    assert_eq!(metadata.flags, ["noStrict"]);
    assert_eq!(metadata.features, ["let"]);
}

#[test]
fn parses_multiline_metadata_lists() {
    let metadata = parse_metadata(
        "/*---\nflags: [\n  noStrict\n]\nfeatures: [\n  Array.prototype.map,\n  Symbol.iterator\n]\nincludes: [\n  assert.js\n]\n---*/\n",
    );
    assert_eq!(metadata.flags, ["noStrict"]);
    assert_eq!(
        metadata.features,
        ["Array.prototype.map", "Symbol.iterator"]
    );
    assert_eq!(metadata.includes, ["assert.js"]);
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
    assert_eq!(skip_reason(&metadata), None);

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
fn captures_test262_output() {
    let case = TestCase {
        path: "output.js".into(),
        source: "console.log('a', 1);".into(),
        metadata: Metadata::default(),
    };
    let report = run_case(&case);
    assert_eq!(report.outcome, Outcome::Pass);
    assert_eq!(report.output, "a 1\n");
}

#[test]
fn supports_error_construction_and_throws_assertions() {
    let case = TestCase {
        path: "errors.js".into(),
        source: "let error = new TypeError('x'); assert.sameValue(error.name, 'TypeError'); assert.throws(TypeError, function () { null.x; });".into(),
        metadata: Metadata::default(),
    };
    assert_eq!(run_case(&case).outcome, Outcome::Pass);
}

#[test]
fn supports_compare_and_deep_assertions() {
    let case = TestCase {
        path: "compare.js".into(),
        source: "assert.compareArray([1, 2], [1, 2]); assert.deepEqual({ a: 1 }, { a: 1 });".into(),
        metadata: Metadata::default(),
    };
    assert_eq!(run_case(&case).outcome, Outcome::Pass);
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
fn validates_negative_phase() {
    let parse_case = TestCase {
        path: "negative-parse.js".into(),
        source: "if (true) let x;".into(),
        metadata: Metadata {
            negative: Some(Negative {
                phase: "parse".into(),
                kind: "SyntaxError".into(),
            }),
            ..Metadata::default()
        },
    };
    assert_eq!(run_case(&parse_case).outcome, Outcome::Pass);

    let runtime_case = TestCase {
        path: "negative-runtime.js".into(),
        source: "let x = undefined; x.y;".into(),
        metadata: Metadata {
            negative: Some(Negative {
                phase: "runtime".into(),
                kind: "TypeError".into(),
            }),
            ..Metadata::default()
        },
    };
    assert_eq!(run_case(&runtime_case).outcome, Outcome::Pass);

    let wrong_phase = TestCase {
        path: "negative-wrong-phase.js".into(),
        source: "$DONOTEVALUATE();".into(),
        metadata: Metadata {
            negative: Some(Negative {
                phase: "parse".into(),
                kind: "SyntaxError".into(),
            }),
            ..Metadata::default()
        },
    };
    assert_eq!(run_case(&wrong_phase).outcome, Outcome::Fail);
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
