use keeper_policy::{
    PermissionRuleAction, PermissionRuleConjunctionEvaluation, PermissionRuleEvaluation,
    PermissionRulePrefix, PermissionRuleScope, PermissionRuleSpec, canonical_process_subject,
    conservative_shell_command_segments, evaluate_permission_rule_conjunction,
    evaluate_permission_rules, permission_rule_matches,
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

#[test]
fn conservative_shell_segmenter_accepts_only_simple_tokens_and_pinned_operators() {
    assert_eq!(
        conservative_shell_command_segments("cargo test"),
        Some(vec!["cargo test".to_owned()])
    );
    assert_eq!(
        conservative_shell_command_segments(
            "cargo   test\t-p kernel-policy && rustc --version || node --version; python -V | cargo check"
        ),
        Some(vec![
            "cargo test -p kernel-policy".to_owned(),
            "rustc --version".to_owned(),
            "node --version".to_owned(),
            "python -V".to_owned(),
            "cargo check".to_owned(),
        ])
    );
}

#[test]
fn conservative_shell_segmenter_fails_closed_on_ambiguous_constructs() {
    let cases = [
        ("quoted", "cargo test --features 'fast'"),
        ("double quoted", "cargo test --features \"fast\""),
        ("command substitution", "cargo test $(touch marker)"),
        ("backtick substitution", "cargo test `touch marker`"),
        ("redirect output", "cargo test > result.txt"),
        ("redirect input", "cargo test < input.txt"),
        ("here document", "cargo test <<EOF"),
        ("variable expansion", "cargo test $TARGET"),
        ("windows variable expansion", "cargo test %TARGET%"),
        ("wildcard", "cargo test crates/*"),
        ("question wildcard", "cargo test crate?"),
        ("bracket wildcard", "cargo test crate[12]"),
        ("escape", "cargo\\ test"),
        ("windows escape", "cargo^ test"),
        ("newline", "cargo test\nrm -rf x"),
        ("background operator", "cargo test & rm -rf x"),
        ("leading operator", "; cargo test"),
        ("trailing operator", "cargo test &&"),
        ("empty segment", "cargo test || ; rm -rf x"),
        ("environment assignment", "MODE=fast cargo test"),
        ("append environment assignment", "MODE+=fast cargo test"),
        ("environment export", "export MODE=fast"),
        ("windows environment mutation", "set MODE=fast"),
        ("compound time keyword", "time cargo test"),
        ("windows command launcher", "start cargo test"),
        ("inline sh", "sh -c \"cargo test && rm -rf x\""),
        ("inline bash", "bash -c cargo"),
        ("inline bash executable", "bash.exe -c cargo"),
        ("inline cmd", "cmd /c cargo test"),
    ];

    for (label, command) in cases {
        assert_eq!(
            conservative_shell_command_segments(command),
            None,
            "{label} must require human confirmation: {command:?}"
        );
    }
}

#[test]
fn conjunctive_allow_proves_normal_cargo_test_and_requires_every_segment() {
    let cargo_allow = rule(
        "cargo-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "cargo test",
        PermissionRuleAction::Allow,
    );
    let rustc_allow = rule(
        "rustc-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "rustc --version",
        PermissionRuleAction::Allow,
    );

    let cargo_only = conservative_shell_command_segments("cargo test").unwrap();
    match evaluate_permission_rule_conjunction(
        PermissionRulePrefix::Process,
        &cargo_only,
        std::slice::from_ref(&cargo_allow),
    ) {
        PermissionRuleConjunctionEvaluation::Allow(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].segment_index, 0);
            assert_eq!(matched[0].matched.rule.rule_id, "cargo-allow");
            assert_eq!(matched[0].matched.match_subject, "cargo test");
        }
        other => panic!("normal cargo test must be allowed, got {other:?}"),
    }

    let mixed = conservative_shell_command_segments("cargo test && rustc --version").unwrap();
    match evaluate_permission_rule_conjunction(
        PermissionRulePrefix::Process,
        &mixed,
        &[cargo_allow.clone(), rustc_allow],
    ) {
        PermissionRuleConjunctionEvaluation::Allow(matched) => assert_eq!(
            matched
                .iter()
                .map(|entry| (entry.segment_index, entry.matched.rule.rule_id.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "cargo-allow"), (1, "rustc-allow")]
        ),
        other => panic!("each segment has an allow, got {other:?}"),
    }

    let dangerous = conservative_shell_command_segments("cargo test; rm -rf x").unwrap();
    assert!(matches!(
        evaluate_permission_rule_conjunction(
            PermissionRulePrefix::Process,
            &dangerous,
            &[cargo_allow],
        ),
        PermissionRuleConjunctionEvaluation::RequireConfirmation
    ));
}

#[test]
fn deny_in_any_segment_beats_allows_and_an_earlier_unmatched_segment() {
    let subjects = conservative_shell_command_segments("unknown command; cargo test; rm -rf x")
        .expect("simple adversarial chain is reliably segmented");
    let rules = [
        rule(
            "cargo-allow",
            PermissionRuleScope::Global,
            PermissionRulePrefix::Process,
            "cargo test",
            PermissionRuleAction::Allow,
        ),
        rule(
            "rm-deny",
            PermissionRuleScope::Project,
            PermissionRulePrefix::Process,
            "rm -rf",
            PermissionRuleAction::Deny,
        ),
    ];

    match evaluate_permission_rule_conjunction(PermissionRulePrefix::Process, &subjects, &rules) {
        PermissionRuleConjunctionEvaluation::Deny(matched) => {
            assert_eq!(matched.segment_index, 2);
            assert_eq!(matched.matched.rule.rule_id, "rm-deny");
            assert_eq!(matched.matched.match_subject, "rm -rf x");
        }
        other => panic!("deny must win across the whole chain, got {other:?}"),
    }
}

#[test]
fn corrupt_project_allow_cannot_satisfy_a_segmented_conjunction() {
    let subjects = conservative_shell_command_segments("cargo test").unwrap();
    let rules = [rule(
        "project-allow",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Process,
        "cargo test",
        PermissionRuleAction::Allow,
    )];

    assert!(matches!(
        evaluate_permission_rule_conjunction(PermissionRulePrefix::Process, &subjects, &rules),
        PermissionRuleConjunctionEvaluation::RequireConfirmation
    ));
}
