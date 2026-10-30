use std::path::Path;

use kagura_conformance::{Outcome, discover, run_case};

#[test]
fn runs_vendored_test262_negative_parse_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/conformance/test262");
    let cases = discover(&root).unwrap();
    assert_eq!(cases.len(), 4);
    for case in &cases {
        let report = run_case(case);
        assert_eq!(
            report.outcome,
            Outcome::Pass,
            "{}: {}",
            report.path.display(),
            report.message
        );
    }
}
