//! Host half of the cross-language process-subject conformance check.
//!
//! The renderer carries its own implementation of this encoding
//! (`apps/desktop/src/features/permissions/processSubject.ts`) because it has
//! to suggest and display rule patterns without calling the host. Two
//! implementations of one wire format is how a seam rots: each side's own
//! tests stay green while the pair drifts apart.
//!
//! Both sides read the same vector file, so drift on either side turns a test
//! red here or in `apps/desktop/tests/process-subject-conformance.test.ts`.

use keeper_policy::{canonical_process_subject, decode_process_subject};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/process-subject-v1.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("conformance fixture is valid JSON")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("argv is an array")
        .iter()
        .map(|item| item.as_str().expect("argv element is a string").to_owned())
        .collect()
}

#[test]
fn every_vector_encodes_to_the_shared_subject() {
    let fixture = fixture();
    let vectors = fixture["vectors"].as_array().expect("vectors array");
    assert!(
        vectors.len() >= 10,
        "the fixture lost vectors — conformance is only as wide as this file"
    );

    for vector in vectors {
        let name = vector["name"].as_str().expect("vector name");
        let program = vector["program"].as_str().expect("program");
        let argv = strings(&vector["argv"]);
        let expected = vector["subject"].as_str().expect("subject");

        assert_eq!(
            canonical_process_subject(program, &argv),
            expected,
            "encoding drifted for vector: {name}"
        );
    }
}

#[test]
fn every_vector_decodes_back_to_program_and_argv() {
    for vector in fixture()["vectors"].as_array().expect("vectors array") {
        let name = vector["name"].as_str().expect("vector name");
        let program = vector["program"].as_str().expect("program");
        let argv = strings(&vector["argv"]);
        let subject = vector["subject"].as_str().expect("subject");

        let decoded =
            decode_process_subject(subject).unwrap_or_else(|| panic!("vector must decode: {name}"));
        let mut expected = vec![program.to_owned()];
        expected.extend(argv);
        assert_eq!(decoded, expected, "decoding drifted for vector: {name}");
    }
}

#[test]
fn undecodable_vectors_stay_undecodable() {
    let fixture = fixture();
    let cases = fixture["undecodable"]
        .as_array()
        .expect("undecodable array");
    assert!(
        cases.len() >= 10,
        "the fixture lost negative cases — fail-closed is the half that matters"
    );

    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let subject = case["subject"].as_str().expect("subject");
        assert!(
            decode_process_subject(subject).is_none(),
            "must not decode (fail-closed): {name}"
        );
    }
}

#[test]
fn the_ea02_collision_pair_has_two_distinct_subjects() {
    // The finding in one assertion: these two argv values are different
    // commands and must never share a rule subject.
    let separate = canonical_process_subject("node", &["safe.js".into(), "--version".into()]);
    let joined = canonical_process_subject("node", &["safe.js --version".into()]);
    assert_ne!(separate, joined);
}
