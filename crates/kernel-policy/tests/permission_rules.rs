use keeper_policy::{
    PermissionRuleAction, PermissionRuleConjunctionEvaluation, PermissionRuleEvaluation,
    PermissionRulePrefix, PermissionRuleScope, PermissionRuleSpec, canonical_process_subject,
    conservative_shell_command_segments, decode_process_subject,
    evaluate_permission_rule_conjunction, evaluate_permission_rules,
    example_also_allowed_process_subject, is_overbroad_process_allow_pattern,
    path_deny_uses_identity_case_fold, permission_rule_matches,
    process_subject_confirmed_for_allow, suggest_process_allow_pattern,
};

/// Build the v1 process subject for tests that exercise structured argv.
fn proc(program: &str, argv: &[&str]) -> String {
    let argv = argv.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    canonical_process_subject(program, &argv)
}

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

fn assert_decoded(subject: &str, expected: &[&str]) {
    let decoded = decode_process_subject(subject).expect("decode");
    let as_str: Vec<&str> = decoded.iter().map(String::as_str).collect();
    assert_eq!(as_str, expected);
}

#[test]
fn canonical_process_subject_is_injective_and_reversible() {
    // Empty argv stays the bare program label (receipt validation compatible).
    assert_eq!(canonical_process_subject("node", &[]), "node");
    assert_decoded("node", &["node"]);

    let cargo = canonical_process_subject(
        "cargo",
        &["test".into(), "-p".into(), "kernel-policy".into()],
    );
    // Must NOT be a bare space-join (EA-02 regression surface).
    assert_ne!(cargo, "cargo test -p kernel-policy");
    assert!(cargo.starts_with("cargo "));
    assert_decoded(&cargo, &["cargo", "test", "-p", "kernel-policy"]);

    let spaced = canonical_process_subject("python", &["two words".into(), "a/b".into()]);
    assert_ne!(spaced, "python two words a/b");
    assert_decoded(&spaced, &["python", "two words", "a/b"]);
}

#[test]
fn process_subject_collision_matrix_keeps_argv_boundaries() {
    // EA-02 reproduction: previously both flattened to "node safe.js --version".
    let two_args = canonical_process_subject("node", &["safe.js".into(), "--version".into()]);
    let one_arg = canonical_process_subject("node", &["safe.js --version".into()]);
    assert_ne!(two_args, one_arg);
    assert_decoded(&two_args, &["node", "safe.js", "--version"]);
    assert_decoded(&one_arg, &["node", "safe.js --version"]);

    // Allow rule saved against the two-arg shape must not approve the one-arg shape.
    let allow_two = rule(
        "allow-two",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "node safe.js *",
        PermissionRuleAction::Allow,
    );
    match evaluate_permission_rules(
        PermissionRulePrefix::Process,
        &two_args,
        &[allow_two.clone()],
    ) {
        PermissionRuleEvaluation::Allow(matched) => {
            assert_eq!(matched.match_subject, two_args);
        }
        other => panic!("two-arg subject must allow under node safe.js *, got {other:?}"),
    }
    assert!(matches!(
        evaluate_permission_rules(PermissionRulePrefix::Process, &one_arg, &[allow_two]),
        PermissionRuleEvaluation::RequireConfirmation
    ));

    // Collision matrix: spaces, empty, consecutive spaces, quotes, backslash, unicode.
    let matrix: Vec<(&str, Vec<String>)> = vec![
        ("space", vec!["two words".into()]),
        ("empty", vec!["".into()]),
        ("multi-space", vec!["a  b".into(), "c".into()]),
        ("quotes", vec!["say\"hi".into(), "'x'".into()]),
        ("backslash", vec!["a\\b".into(), "c".into()]),
        ("unicode", vec!["café".into(), "日本語".into()]),
        ("colon-len-confusion", vec!["1:x".into(), "x".into()]),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (label, argv) in matrix {
        let subject = canonical_process_subject("prog", &argv);
        assert!(
            seen.insert(subject.clone()),
            "{label}: subject must be unique, got {subject:?}"
        );
        let decoded = decode_process_subject(&subject).expect(label);
        assert_eq!(decoded[0], "prog", "{label}");
        assert_eq!(&decoded[1..], argv.as_slice(), "{label}");
    }

    // Cross-collision: ["a","b"] vs ["a b"] vs ["a","", "b"] etc.
    let a = canonical_process_subject("p", &["a".into(), "b".into()]);
    let b = canonical_process_subject("p", &["a b".into()]);
    let c = canonical_process_subject("p", &["a".into(), "".into(), "b".into()]);
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(b, c);
}

#[test]
fn legacy_flat_process_subjects_fail_closed_for_allow() {
    // Old durable shape: space-joined subject string. Must not decode, must not
    // auto-allow even under a broad pattern, and must not confirm.
    let legacy = "node safe.js --version";
    assert!(decode_process_subject(legacy).is_none());
    assert!(!process_subject_confirmed_for_allow(legacy));
    assert!(!permission_rule_matches(
        PermissionRulePrefix::Process,
        "node*",
        legacy
    ));
    let allow = rule(
        "legacy-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "node*",
        PermissionRuleAction::Allow,
    );
    assert!(matches!(
        evaluate_permission_rules(PermissionRulePrefix::Process, legacy, &[allow]),
        PermissionRuleEvaluation::RequireConfirmation
    ));
}

#[test]
fn pure_prefix_glob_and_subject_matrix_is_pinned() {
    struct Case {
        prefix: PermissionRulePrefix,
        pattern: &'static str,
        subject: String,
        expected: bool,
        label: &'static str,
    }

    // Process subjects must be v1-encoded; bijective argv still match via the
    // historical space-join surface inside process_rule_matches.
    let cases = [
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "",
            subject: proc("cargo", &["--version"]),
            expected: true,
            label: "bare process matches all",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo*",
            subject: proc("cargo", &["--version"]),
            expected: true,
            label: "Bash(cargo*) golden case",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo",
            subject: proc("cargo", &["test"]),
            expected: true,
            label: "literal process prefix",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo",
            subject: proc("cargo-nextest", &["run"]),
            expected: true,
            label: "known no-word-boundary behavior",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo *",
            subject: proc("cargo", &[]),
            expected: false,
            label: "space-star requires separator",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo *",
            subject: proc("cargo", &["test", "-p", "a/b"]),
            expected: true,
            label: "process star crosses spaces and slash",
        },
        Case {
            prefix: PermissionRulePrefix::Process,
            pattern: "cargo?",
            subject: proc("cargoX", &[]),
            expected: false,
            label: "question mark is literal",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/*",
            subject: "src/lib.rs".into(),
            expected: true,
            label: "path star matches one segment",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/*",
            subject: "src/nested/lib.rs".into(),
            expected: false,
            label: "path star does not cross slash",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/**",
            subject: "src/nested/lib.rs".into(),
            expected: true,
            label: "path double-star crosses slash",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "src/**/*.rs",
            subject: "src/lib.rs".into(),
            expected: true,
            label: "double-star slash matches zero directories",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "src/**/*.rs",
            subject: "src/a/b/lib.rs".into(),
            expected: true,
            label: "double-star slash matches multiple directories",
        },
        Case {
            prefix: PermissionRulePrefix::Write,
            pattern: "src/**/*.rs",
            subject: "src/a/b/lib.ts".into(),
            expected: false,
            label: "literal suffix remains exact",
        },
        Case {
            // Raw matcher stays case-sensitive on every platform. EA-01 deny
            // identity fold is evaluate-time only (allow remains strict).
            // Old contract "matching is case-sensitive on every platform" was
            // wrong for Windows *deny* because OS object identity is not
            // case-sensitive — see windows_write_edit_deny_folds_path_identity.
            prefix: PermissionRulePrefix::Write,
            pattern: "SRC/**",
            subject: "src/lib.rs".into(),
            expected: false,
            label: "raw path matcher is case-sensitive (deny fold is evaluate-time)",
        },
        Case {
            prefix: PermissionRulePrefix::Edit,
            pattern: "src/lib.rs",
            subject: "src/lib.rs.bak".into(),
            expected: false,
            label: "path literal is whole-subject, not prefix",
        },
    ];

    for case in cases {
        assert_eq!(
            permission_rule_matches(case.prefix, case.pattern, &case.subject),
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
fn windows_write_edit_deny_folds_path_identity_allow_stays_strict() {
    // EA-01: deny must catch case aliases of the same project-relative object
    // on Windows; allow stays case-sensitive (narrower auto-approve).
    let deny = rule(
        "deny-blocked",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Edit,
        "blocked/**",
        PermissionRuleAction::Deny,
    );
    let allow = rule(
        "allow-blocked",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Edit,
        "blocked/**",
        PermissionRuleAction::Allow,
    );
    let alias = "BLOCKED/SECRET.TXT";

    if path_deny_uses_identity_case_fold() {
        match evaluate_permission_rules(PermissionRulePrefix::Edit, alias, &[deny.clone()]) {
            PermissionRuleEvaluation::Deny(matched) => {
                assert_eq!(matched.rule.rule_id, "deny-blocked");
                assert_eq!(matched.match_subject, alias);
            }
            other => panic!("Windows deny must fold path identity, got {other:?}"),
        }
        // Allow does not fold: case mismatch requires confirmation (mode may
        // still auto-approve; deny is what stops the bypass).
        assert!(matches!(
            evaluate_permission_rules(PermissionRulePrefix::Edit, alias, &[allow]),
            PermissionRuleEvaluation::RequireConfirmation
        ));
        // Create-class deny (write prefix) likewise folds.
        let write_deny = rule(
            "deny-write",
            PermissionRuleScope::Project,
            PermissionRulePrefix::Write,
            "blocked/**",
            PermissionRuleAction::Deny,
        );
        assert!(matches!(
            evaluate_permission_rules(
                PermissionRulePrefix::Write,
                "BLOCKED/new.txt",
                &[write_deny]
            ),
            PermissionRuleEvaluation::Deny(_)
        ));
    } else {
        // Unix: case-sensitive deny — alias does not hit `blocked/**`.
        assert!(matches!(
            evaluate_permission_rules(PermissionRulePrefix::Edit, alias, &[deny]),
            PermissionRuleEvaluation::RequireConfirmation
        ));
        assert!(
            !path_deny_uses_identity_case_fold(),
            "non-Windows must document case-sensitive deny"
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

    let subject = proc("cargo", &["test", "-p", "keeper-policy"]);
    for rules in [vec![allow.clone(), deny.clone()], vec![deny.clone(), allow]] {
        match evaluate_permission_rules(PermissionRulePrefix::Process, &subject, &rules) {
            PermissionRuleEvaluation::Deny(matched) => {
                assert_eq!(matched.rule.rule_id, deny.rule_id);
                assert_eq!(matched.match_subject, subject);
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

/// Why the SQLite authority must never load an empty-pattern row.
///
/// Policy is deliberately literal here: an empty pattern matches everything. A
/// blank `global process allow` row would therefore auto-approve every command,
/// which is what makes quarantining such a row (rather than loading it) the only
/// safe reading of a corrupt store.
#[test]
fn an_empty_pattern_matches_every_subject_in_its_class() {
    for prefix in [
        PermissionRulePrefix::Process,
        PermissionRulePrefix::Write,
        PermissionRulePrefix::Edit,
    ] {
        assert!(permission_rule_matches(
            prefix,
            "",
            &proc("rm", &["-rf", "/"])
        ));
        assert!(permission_rule_matches(prefix, "", "project/.ssh/id_rsa"));
        assert!(permission_rule_matches(prefix, "", ""));
    }

    let blank_allow = [rule(
        "blank-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "",
        PermissionRuleAction::Allow,
    )];
    assert!(matches!(
        evaluate_permission_rules(
            PermissionRulePrefix::Process,
            &proc("git", &["push", "--force"]),
            &blank_allow
        ),
        PermissionRuleEvaluation::Allow(_)
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
        evaluate_permission_rules(
            PermissionRulePrefix::Process,
            &proc("cargo", &["check"]),
            &rules
        ),
        PermissionRuleEvaluation::RequireConfirmation
    ));
}

#[test]
fn conservative_shell_segmenter_accepts_only_simple_tokens_and_pinned_operators() {
    assert_eq!(
        conservative_shell_command_segments("cargo test"),
        Some(vec![proc("cargo", &["test"])])
    );
    assert_eq!(
        conservative_shell_command_segments(
            "cargo   test\t-p kernel-policy && rustc --version || node --version; python -V | cargo check"
        ),
        Some(vec![
            proc("cargo", &["test", "-p", "kernel-policy"]),
            proc("rustc", &["--version"]),
            proc("node", &["--version"]),
            proc("python", &["-V"]),
            proc("cargo", &["check"]),
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
    let cargo_subject = proc("cargo", &["test"]);
    match evaluate_permission_rule_conjunction(
        PermissionRulePrefix::Process,
        &cargo_only,
        std::slice::from_ref(&cargo_allow),
    ) {
        PermissionRuleConjunctionEvaluation::Allow(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].segment_index, 0);
            assert_eq!(matched[0].matched.rule.rule_id, "cargo-allow");
            assert_eq!(matched[0].matched.match_subject, cargo_subject);
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
            assert_eq!(matched.matched.match_subject, proc("rm", &["-rf", "x"]));
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

fn process_allow(pattern: &str) -> PermissionRuleSpec {
    rule(
        "process-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        pattern,
        PermissionRuleAction::Allow,
    )
}

fn assert_process_allow(subject: &str, pattern: &str) {
    match evaluate_permission_rules(
        PermissionRulePrefix::Process,
        subject,
        &[process_allow(pattern)],
    ) {
        PermissionRuleEvaluation::Allow(matched) => {
            assert_eq!(matched.match_subject, subject);
            assert_eq!(matched.rule.pattern, pattern);
        }
        other => panic!("expected allow for {subject:?} under {pattern:?}, got {other:?}"),
    }
}

fn assert_process_requires_confirmation(subject: &str, pattern: &str) {
    assert!(
        matches!(
            evaluate_permission_rules(
                PermissionRulePrefix::Process,
                subject,
                &[process_allow(pattern)],
            ),
            PermissionRuleEvaluation::RequireConfirmation
        ),
        "expected require-confirmation for {subject:?} under {pattern:?}"
    );
}

#[test]
fn opaque_eval_shapes_do_not_auto_allow_under_broad_program_rules() {
    // Matching allow alone is not enough: opaque evaluation shapes must prompt.
    let adversarial = [
        (proc("node", &["-e", "code"]), "node*"),
        (proc("node", &["--eval", "code"]), "node*"),
        (proc("node.exe", &["-e", "code"]), "node*"),
        (proc("python", &["-c", "code"]), "python*"),
        (proc("python3", &["-c", "code"]), "python*"),
        (proc("/usr/bin/python3", &["-c", "code"]), "python*"),
        (proc("perl", &["-e", "code"]), "perl*"),
        (proc("ruby", &["-e", "code"]), "ruby*"),
        (proc("php", &["-r", "code"]), "php*"),
        (proc("deno", &["eval", "code"]), "deno*"),
        (proc("sh", &["-c", "cargo test"]), "sh*"),
        (proc("bash", &["-c", "cargo test"]), "bash*"),
        (proc("bash", &["-xc", "cargo test"]), "bash*"),
        (proc("env", &["node", "--version"]), "env*"),
        (proc("xargs", &["rm", "-rf", "/"]), "xargs*"),
        (proc("find", &[".", "-exec", "rm", "{}", ";"]), "find*"),
        (proc("find", &[".", "-execdir", "rm", "{}", ";"]), "find*"),
        (proc("npx", &["cowsay", "hi"]), "npx*"),
        (proc("uvx", &["ruff", "check"]), "uvx*"),
        (proc("bunx", &["cowsay", "hi"]), "bunx*"),
    ];

    for (subject, pattern) in adversarial {
        assert_process_requires_confirmation(&subject, pattern);
        assert!(
            !process_subject_confirmed_for_allow(&subject),
            "shape must be unconfirmed: {subject}"
        );
    }
}

#[test]
fn normal_toolchain_commands_still_auto_allow_under_matching_rules() {
    // Positive direction: a reject-everything implementation would fail these.
    let benign = [
        (proc("cargo", &["test"]), "cargo*"),
        (
            proc("cargo", &["test", "-p", "keeper-policy"]),
            "cargo test*",
        ),
        (proc("npm", &["run", "build"]), "npm*"),
        (proc("git", &["status"]), "git*"),
        (
            proc("git", &["-c", "advice.detachedHead=false", "status"]),
            "git*",
        ),
        (proc("rustc", &["--version"]), "rustc*"),
        (proc("node", &["--version"]), "node*"),
        (proc("python", &["--version"]), "python*"),
        (proc("ls", &["-c"]), "ls*"),
        (proc("ps", &["-ef"]), "ps*"),
        (proc("grep", &["-e", "pattern", "file.txt"]), "grep*"),
        (proc("find", &[".", "-name", "*.rs"]), "find*"),
    ];

    for (subject, pattern) in benign {
        assert!(
            process_subject_confirmed_for_allow(&subject),
            "shape must be confirmed: {subject}"
        );
        assert_process_allow(&subject, pattern);
    }
}

#[test]
fn deny_still_wins_on_opaque_eval_shapes() {
    let allow = process_allow("node*");
    let deny = rule(
        "node-deny",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Process,
        "node -e",
        PermissionRuleAction::Deny,
    );
    let subject = proc("node", &["-e", "code"]);

    match evaluate_permission_rules(PermissionRulePrefix::Process, &subject, &[allow, deny]) {
        PermissionRuleEvaluation::Deny(matched) => {
            assert_eq!(matched.rule.rule_id, "node-deny");
        }
        other => panic!("deny must beat allow even for opaque shapes, got {other:?}"),
    }
}

#[test]
fn opaque_eval_segment_blocks_conjunctive_allow_of_a_chain() {
    let cargo_allow = process_allow("cargo*");
    let node_allow = rule(
        "node-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Process,
        "node*",
        PermissionRuleAction::Allow,
    );

    // `node -e` cannot be confirmed, so the whole conjunction must prompt even
    // though `cargo test` alone would auto-allow.
    let chain = conservative_shell_command_segments("cargo test && node -e code")
        .expect("simple chain segments");
    assert!(matches!(
        evaluate_permission_rule_conjunction(
            PermissionRulePrefix::Process,
            &chain,
            &[cargo_allow, node_allow],
        ),
        PermissionRuleConjunctionEvaluation::RequireConfirmation
    ));

    // Positive control: both confirmed segments still allow.
    let ok = conservative_shell_command_segments("cargo test && node --version").unwrap();
    match evaluate_permission_rule_conjunction(
        PermissionRulePrefix::Process,
        &ok,
        &[process_allow("cargo*"), process_allow("node*")],
    ) {
        PermissionRuleConjunctionEvaluation::Allow(matched) => assert_eq!(matched.len(), 2),
        other => panic!("confirmed chain must allow, got {other:?}"),
    }
}

#[test]
fn write_and_edit_allows_are_not_gated_by_process_shape() {
    let rules = [rule(
        "edit-allow",
        PermissionRuleScope::Global,
        PermissionRulePrefix::Edit,
        "src/**",
        PermissionRuleAction::Allow,
    )];

    match evaluate_permission_rules(PermissionRulePrefix::Edit, "src/lib.rs", &rules) {
        PermissionRuleEvaluation::Allow(matched) => {
            assert_eq!(matched.match_subject, "src/lib.rs");
        }
        other => panic!("edit allow must not consult process shape, got {other:?}"),
    }
}

/// Approving `node build.js` must not casually suggest a pattern that would
/// auto-allow a different script. This is the known mis-allow path: a one-click
/// "always allow" that persists `node*` / `node *`.
#[test]
fn suggested_allow_pattern_from_node_build_js_does_not_auto_allow_a_different_script() {
    let approved = proc("node", &["build.js"]);
    let suggested = suggest_process_allow_pattern(&approved);
    let other = proc("node", &["anything-else.js"]);

    assert!(
        !is_overbroad_process_allow_pattern(&suggested),
        "suggestion must not be an overbroad allow pattern, got {suggested:?}"
    );
    assert_ne!(suggested, "node*");
    assert_ne!(suggested, "node *");
    assert_ne!(suggested, "node");

    // The approved command itself must still auto-allow under the suggestion.
    assert_process_allow(&approved, &suggested);

    // A semantically different argv must still prompt — not silently match.
    assert!(
        process_subject_confirmed_for_allow(&other),
        "the contrast script must be a confirmable shape so a match would auto-allow"
    );
    assert_process_requires_confirmation(&other, &suggested);
}

#[test]
fn overbroad_process_allow_pattern_classification_is_a_pure_table() {
    let overbroad = [
        "",
        "*",
        "node",
        "node*",
        "node *",
        "node **",
        "no*de",
        "*node",
        "/usr/bin/node *",
        "C:\\nodejs\\node.exe*",
    ];
    for pattern in overbroad {
        assert!(
            is_overbroad_process_allow_pattern(pattern),
            "expected overbroad: {pattern:?}"
        );
    }

    let narrow = [
        "node build.js",
        "node build.js *",
        "cargo test *",
        "cargo test -p keeper-policy",
    ];
    for pattern in narrow {
        assert!(
            !is_overbroad_process_allow_pattern(pattern),
            "expected not overbroad: {pattern:?}"
        );
    }

    // Exact v1 subjects pin one argv and are never classified overbroad.
    let exact = proc("node", &["build.js"]);
    assert!(!is_overbroad_process_allow_pattern(&exact));
}

#[test]
fn stored_overbroad_allow_still_auto_allows_a_different_confirmed_script() {
    // Backward compatible: already-saved wide rules keep their match surface.
    // This ticket does not rewrite or ignore them.
    let other = proc("node", &["anything-else.js"]);
    assert!(process_subject_confirmed_for_allow(&other));
    assert_process_allow(&other, "node*");
    assert_process_allow(&other, "node *");
    assert_process_allow(&other, "node");
}

#[test]
fn deny_match_surface_is_not_narrowed_for_broad_process_patterns() {
    let other = proc("node", &["anything-else.js"]);
    let deny_star = rule(
        "deny-node-star",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Process,
        "node*",
        PermissionRuleAction::Deny,
    );
    let deny_space = rule(
        "deny-node-space",
        PermissionRuleScope::Project,
        PermissionRulePrefix::Process,
        "node *",
        PermissionRuleAction::Deny,
    );
    for deny in [deny_star, deny_space] {
        match evaluate_permission_rules(PermissionRulePrefix::Process, &other, &[deny.clone()]) {
            PermissionRuleEvaluation::Deny(matched) => {
                assert_eq!(matched.rule.pattern, deny.pattern);
            }
            other_eval => panic!(
                "broad deny must still hit a different script, pattern {:?}, got {other_eval:?}",
                deny.pattern
            ),
        }
    }
}

#[test]
fn overbroad_allow_example_is_a_real_auto_allow_derived_from_the_subject() {
    let approved = proc("node", &["build.js"]);
    let example = example_also_allowed_process_subject("node *", &approved)
        .expect("overbroad pattern must yield a real contrast subject");
    assert_ne!(example, approved);
    let decoded = decode_process_subject(&example).expect("contrast must decode");
    assert_eq!(decoded[0], "node");
    assert_ne!(decoded.get(1).map(String::as_str), Some("build.js"));
    assert!(process_subject_confirmed_for_allow(&example));
    assert_process_allow(&example, "node *");
    assert_process_allow(&example, "node*");

    // Narrow suggestions must not invent a false "also allowed" example.
    let suggested = suggest_process_allow_pattern(&approved);
    assert!(example_also_allowed_process_subject(&suggested, &approved).is_none());
}
