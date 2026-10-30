use std::path::Path;

use kagura_conformance::{Outcome, discover, run_case};

#[test]
fn runs_vendored_test262_negative_parse_case() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/conformance/test262");
    let cases = discover(&root).unwrap();
    assert_eq!(cases.len(), 1);
    let report = run_case(&cases[0]);
    assert_eq!(report.outcome, Outcome::Pass, "{}", report.message);
}
