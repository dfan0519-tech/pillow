use keeper_policy::{
    PermissionRuleAction, PermissionRuleEvaluation, PermissionRulePrefix, PermissionRuleScope,
    PermissionRuleSpec, canonical_process_subject, evaluate_permission_rules,
    permission_rule_matches,
};

fn rule(
    id: &str,
    scope: PermissionRuleScope,
    prefix: PermissionRulePrefix,
    pattern: &str,
    action: PermissionRuleAction,
) -> PermissionRuleSpec {
    PermissionRuleSpec {
        rule_id: id.to_owned(),
        scope,
        project_id: (scope == PermissionRuleScope::Project).then(|| "project-a".to_owned()),
        prefix,
        pattern: pattern.to_owned(),
        action,
        content_digest: format!("sha256:{id:0<64}"),
    }
}

#[test]
fn canonical_process_subject_is_literal_space_join_without_shell_quoting() {
    assert_eq!(
        canonical_process_subject(
            "cargo",
            &["test".into(), "-p".into(), "kernel-policy".into()]
        ),
        "cargo test -p kernel-policy"
    );
    assert_eq!(canonical_process_subject("node", &[]), "node");
    assert_eq!(
        canonical_process_subject("python", &["two words".into(), "a/b".into()]),
        "python two words a/b"
    );
}

#[test]
fn pure_prefix_glob_and_subject_matrix_is_pinned() {
    struct Case {
        prefix: PermissionRulePrefix,
        pattern: &'static str,
        subject: &'static str,
        expected: bool,
        label: &'static str,
    }

    let cases = [
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "",
            subject: "cargo --version",
            expected: true,
            label: "bare process matches all",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo*",
            subject: "cargo --version",
            expected: true,
            label: "Bash(cargo*) golden case",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo",
            subject: "cargo test",
            expected: true,
            label: "literal process prefix",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo",
            subject: "cargo-nextest run",
            expected: true,
            label: "known no-word-boundary behavior",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo *",
            subject: "cargo",
            expected: false,
            label: "space-star requires separator",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo *",
            subject: "cargo test -p a/b",
            expected: true,
            label: "process star crosses spaces and slash",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo?",
            subject: "cargoX",
            expected: false,
            label: "question mark is literal",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/*",
            subject: "src/lib.rs",
            expected: true,
            label: "path star matches one segment",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/*",
            subject: "src/nested/lib.rs",
            expected: false,
            label: "path star does not cross slash",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/**",
            subject: "src/nested/lib.rs",
            expected: true,
            label: "path double-star crosses slash",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "src/**/*.rs",
            subject: "src/lib.rs",
            expected: true,
            label: "double-star slash matches zero directories",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "src/**/*.rs",
            subject: "src/a/b/lib.rs",
            expected: true,
            label: "double-star slash matches multiple directories",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "src/**/*.rs",
            subject: "src/a/b/lib.ts",
            expected: false,
            label: "literal suffix remains exact",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "SRC/**",
            subject: "src/lib.rs",
            expected: false,
            label: "matching is case-sensitive on every platform",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/lib.rs",
            subject: "src/lib.rs.bak",
            expected: false,
            label: "path literal is whole-subject, not prefix",
        },
    ];

    for case in cases {
        assert_eq!(
            permission_rule_matches(case.prefix, case.pattern, case.subject),
            case.expected,
            "{}: prefix={:?} pattern={:?} subject={:?}",
            case.label,
            case.prefix,
            case.pattern,
            case.subject
        );
    }
}

#[test]
fn prefix_class_must_match_before_pattern_is_considered() {
    let rules = [rule(
        "allow-process",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "*",
        PermissionRuleAction::Allow,
    )];

    assert!(matches!(
        evaluate_permission_rules(PermissionRulePrefix::Edit, "src/lib.rs", &rules),
        PermissionRuleEvaluation::RequireConfirmation
    ));
}

#[test]
fn deny_beats_allow_regardless_of_input_order() {
    let allow = rule(
        "allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "cargo *",
        PermissionRuleAction::Allow,
    );
    let deny = rule(
        "deny",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Process,
        "cargo test*",
        PermissionRuleAction::Deny,
    );

    for rules in [vec![allow.clone(), deny.clone()], vec![deny.clone(), allow]] {
        match evaluate_permission_rules(
            PermissionRulePrefix::Process,
            "cargo test -p keeper-policy",
            &rules,
        ) {
            PermissionRuleEvaluation::Deny(matched) => {
                assert_eq!(matched.rule.rule_id, deny.rule_id);
                assert_eq!(matched.match_subject, "cargo test -p keeper-policy");
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }
}

#[test]
fn project_allow_is_never_an_authority_even_if_a_corrupt_input_reaches_policy() {
    let rules = [rule(
        "project-allow",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Edit,
        "src/**",
        PermissionRuleAction::Allow,
    )];

    assert!(matches!(
        evaluate_permission_rules(PermissionRulePrefix::Edit, "src/lib.rs", &rules),
        PermissionRuleEvaluation::RequireConfirmation
    ));
}

#[test]
fn no_matching_rule_still_requires_confirmation() {
    let rules = [rule(
        "narrow-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "cargo test *",
        PermissionRuleAction::Allow,
    )];

    assert!(matches!(
        evaluate_permission_rules(PermissionRulePrefix::Process, "cargo check", &rules),
        PermissionRuleEvaluation::RequireConfirmation
    ));
}
