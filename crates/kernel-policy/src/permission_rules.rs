//! Pure permission-rule matching and precedence.
//!
//! Pillow pins a small deterministic wildcard language rather than claiming
//! bit-for-bit Grok compatibility. Only `*` and `**` are operators.
//!
//! # Path identity (write / edit) — platform semantics
//!
//! Rule subjects are portable project-relative paths. The OS may resolve those
//! paths with different case-sensitivity than a byte-wise string compare:
//!
//! | Platform | Deny match | Allow match | Rationale |
//! | --- | --- | --- | --- |
//! | Windows | Case-folded (Unicode lowercase) identity | Case-sensitive | NTFS default is case-insensitive; deny must not miss aliases. Narrower allow is fail-closed for auto-approve. |
//! | non-Windows | Case-sensitive | Case-sensitive | Typical Unix volumes are case-sensitive. Residual: case-insensitive mounts (some macOS APFS) are documented as residual risk. |
//!
//! Fail-closed for deny: when identity is ambiguous under the platform table,
//! deny matching uses the broader (folded) comparison so aliases cannot slip
//! into mode auto-approval.
//!
//! # Process subjects
//!
//! Process subjects use a versioned length-prefix encoding ([`canonical_process_subject`])
//! so argv boundaries are recoverable. Legacy space-joined subjects do not
//! decode and therefore never auto-allow (fail-closed migration).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleScope {
    Global,
    Project,
}

impl PermissionRuleScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "global" => Some(Self::Global),
            "project" => Some(Self::Project),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRulePrefix {
    Process,
    Write,
    Edit,
}

impl PermissionRulePrefix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Write => "write",
            Self::Edit => "edit",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "process" => Some(Self::Process),
            "write" => Some(Self::Write),
            "edit" => Some(Self::Edit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleAction {
    Allow,
    Deny,
}

impl PermissionRuleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// Complete immutable rule body needed for matching and receipt attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuleSpec {
    pub rule_id: String,
    pub scope: PermissionRuleScope,
    pub project_id: Option<String>,
    pub prefix: PermissionRulePrefix,
    pub pattern: String,
    pub action: PermissionRuleAction,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPermissionRule {
    pub rule: PermissionRuleSpec,
    pub match_subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionRuleEvaluation {
    Deny(MatchedPermissionRule),
    Allow(MatchedPermissionRule),
    RequireConfirmation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPermissionRuleSegment {
    pub segment_index: usize,
    pub matched: MatchedPermissionRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionRuleConjunctionEvaluation {
    Deny(MatchedPermissionRuleSegment),
    Allow(Vec<MatchedPermissionRuleSegment>),
    RequireConfirmation,
}

/// Split only shell command shapes whose token and operator boundaries are
/// unambiguous without a shell parser.
///
/// `None` is a security result, not a parse error: callers must require human
/// confirmation. Quotes, expansion, redirection, globbing, escaping, compound
/// grammar and nested shell interpreters are deliberately outside this small
/// accepted language.
pub fn conservative_shell_command_segments(command: &str) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\t' => finish_shell_token(&mut token, &mut tokens),
            ';' => finish_shell_segment(&mut token, &mut tokens, &mut segments)?,
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                finish_shell_segment(&mut token, &mut tokens, &mut segments)?;
            }
            '&' => {
                chars.next_if_eq(&'&')?;
                finish_shell_segment(&mut token, &mut tokens, &mut segments)?;
            }
            '\'' | '"' | '`' | '$' | '\\' | '<' | '>' | '*' | '?' | '[' | ']' | '{' | '}' | '('
            | ')' | '~' | '#' | '%' | '!' | '^' => return None,
            other if other.is_control() || other.is_whitespace() => return None,
            other => token.push(other),
        }
    }

    finish_shell_segment(&mut token, &mut tokens, &mut segments)?;
    Some(segments)
}

/// Evaluate a segmented command as a conjunction of independently authorized
/// subjects. A deny on any segment wins, while allow succeeds only when every
/// segment has its own allow match.
pub fn evaluate_permission_rule_conjunction(
    prefix: PermissionRulePrefix,
    subjects: &[String],
    rules: &[PermissionRuleSpec],
) -> PermissionRuleConjunctionEvaluation {
    if subjects.is_empty() {
        return PermissionRuleConjunctionEvaluation::RequireConfirmation;
    }

    let mut first_deny = None;
    let mut allows = Vec::with_capacity(subjects.len());
    let mut missing_allow = false;
    for (segment_index, subject) in subjects.iter().enumerate() {
        match evaluate_permission_rules(prefix, subject, rules) {
            PermissionRuleEvaluation::Deny(matched) => {
                if first_deny.is_none() {
                    first_deny = Some(MatchedPermissionRuleSegment {
                        segment_index,
                        matched,
                    });
                }
            }
            PermissionRuleEvaluation::Allow(matched) => {
                allows.push(MatchedPermissionRuleSegment {
                    segment_index,
                    matched,
                });
            }
            PermissionRuleEvaluation::RequireConfirmation => missing_allow = true,
        }
    }

    if let Some(matched) = first_deny {
        PermissionRuleConjunctionEvaluation::Deny(matched)
    } else if missing_allow {
        PermissionRuleConjunctionEvaluation::RequireConfirmation
    } else {
        PermissionRuleConjunctionEvaluation::Allow(allows)
    }
}

fn finish_shell_token(token: &mut String, tokens: &mut Vec<String>) {
    if !token.is_empty() {
        tokens.push(std::mem::take(token));
    }
}

fn finish_shell_segment(
    token: &mut String,
    tokens: &mut Vec<String>,
    segments: &mut Vec<String>,
) -> Option<()> {
    finish_shell_token(token, tokens);
    if tokens.is_empty() || !simple_shell_tokens_are_reliable(tokens) {
        return None;
    }
    // Emit the same injective process subject encoding used for structured
    // proposals so shell segments never re-introduce space-join collisions.
    let program = tokens[0].as_str();
    let argv = tokens[1..].to_vec();
    segments.push(canonical_process_subject(program, &argv));
    tokens.clear();
    Some(())
}

fn simple_shell_tokens_are_reliable(tokens: &[String]) -> bool {
    let first = tokens.first().map(String::as_str).unwrap_or_default();
    !looks_like_environment_assignment(first)
        && !matches!(
            first.to_ascii_lowercase().as_str(),
            "env"
                | "eval"
                | "source"
                | "."
                | "call"
                | "if"
                | "then"
                | "else"
                | "elif"
                | "fi"
                | "for"
                | "while"
                | "until"
                | "do"
                | "done"
                | "case"
                | "esac"
                | "function"
                | "select"
                | "in"
                | "time"
                | "coproc"
                | "export"
                | "readonly"
                | "declare"
                | "typeset"
                | "local"
                | "set"
                | "unset"
                | "setlocal"
                | "endlocal"
                | "command"
                | "builtin"
                | "exec"
                | "trap"
                | "alias"
                | "unalias"
                | "start"
                | "goto"
        )
        && !tokens.iter().any(|token| is_shell_interpreter(token))
}

fn looks_like_environment_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    let name = name.strip_suffix('+').unwrap_or(name);
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_shell_interpreter(token: &str) -> bool {
    let basename = token
        .rsplit('/')
        .next()
        .unwrap_or(token)
        .to_ascii_lowercase();
    matches!(
        basename.as_str(),
        "sh" | "sh.exe"
            | "bash"
            | "bash.exe"
            | "dash"
            | "dash.exe"
            | "ash"
            | "ash.exe"
            | "zsh"
            | "zsh.exe"
            | "fish"
            | "fish.exe"
            | "ksh"
            | "ksh.exe"
            | "mksh"
            | "yash"
            | "csh"
            | "tcsh"
            | "xonsh"
            | "nu"
            | "elvish"
            | "cmd"
            | "cmd.exe"
            | "command.com"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
            | "wsl"
            | "wsl.exe"
    )
}

/// Version mark for process subject v1 encoding (ASCII RS + `p1`).
///
/// Placed after `{program} ` so durable rule-receipt validation that requires
/// `match_subject == program || match_subject.starts_with(program + " ")` still
/// holds without widening storage contracts.
const PROCESS_SUBJECT_V1_MARK: &str = "\u{1e}p1";

/// Canonical process subject: injective, reversible encoding of
/// `(program, argv)`.
///
/// # Encoding (v1)
///
/// - Empty argv → `{program}`
/// - Non-empty → `{program} \u{1e}p1` then for each arg `:{hex_byte_len}:{utf8}`
///   where `hex_byte_len` is the lowercase hex UTF-8 byte length of that arg.
///
/// No shell quoting. Argv boundaries are always recoverable via
/// [`decode_process_subject`]. Distinct structured argv values never share a
/// subject (including space-containing, empty, and multi-space args).
pub fn canonical_process_subject(program: &str, argv: &[String]) -> String {
    if argv.is_empty() {
        return program.to_owned();
    }
    let mut subject = String::with_capacity(
        program.len()
            + 1
            + PROCESS_SUBJECT_V1_MARK.len()
            + argv.iter().map(|arg| 1 + 8 + 1 + arg.len()).sum::<usize>(),
    );
    subject.push_str(program);
    subject.push(' ');
    subject.push_str(PROCESS_SUBJECT_V1_MARK);
    for argument in argv {
        subject.push(':');
        subject.push_str(&format!("{:x}", argument.len()));
        subject.push(':');
        subject.push_str(argument);
    }
    subject
}

/// Decode a [`canonical_process_subject`] value into `[program, ...argv]`.
///
/// Returns `None` for legacy space-joined subjects and any malformed payload.
/// Callers must treat `None` as fail-closed (no auto-allow).
pub fn decode_process_subject(subject: &str) -> Option<Vec<String>> {
    if subject.is_empty() || subject.contains('\0') {
        return None;
    }

    // Program-only form (empty argv).
    if !subject.contains(' ') {
        // Reject stray version marks inside a bare program label.
        if subject.contains('\u{1e}') {
            return None;
        }
        return Some(vec![subject.to_owned()]);
    }

    let (program, after_program) = subject.split_once(' ')?;
    if program.is_empty() || program.contains('\u{1e}') {
        return None;
    }
    let Some(mut cursor) = after_program.strip_prefix(PROCESS_SUBJECT_V1_MARK) else {
        // Legacy flat "program arg1 arg2" subjects intentionally do not decode.
        return None;
    };
    if cursor.is_empty() {
        // Mark without args is not a produced form.
        return None;
    }

    let mut tokens = vec![program.to_owned()];
    while !cursor.is_empty() {
        if !cursor.starts_with(':') {
            return None;
        }
        cursor = &cursor[1..];
        let (len_hex, after_len) = cursor.split_once(':')?;
        if len_hex.is_empty() || !len_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        // Reject leading zeros except for the single digit `0` (empty arg).
        if len_hex.len() > 1 && len_hex.starts_with('0') {
            return None;
        }
        let len = usize::from_str_radix(len_hex, 16).ok()?;
        let raw = after_len.as_bytes();
        if raw.len() < len {
            return None;
        }
        let arg = std::str::from_utf8(&raw[..len]).ok()?.to_owned();
        tokens.push(arg);
        cursor = &after_len[len..];
    }
    Some(tokens)
}

/// True when space-joining tokens is a bijection (no empty token, no embedded
/// ASCII space or subject version mark). Used only as a **pattern match surface**
/// for Grok-compatible process globs against simple toolchain argv.
fn bijective_space_join(tokens: &[String]) -> Option<String> {
    if tokens.is_empty() {
        return None;
    }
    if tokens
        .iter()
        .any(|token| token.is_empty() || token.contains(' ') || token.contains('\u{1e}'))
    {
        return None;
    }
    Some(tokens.join(" "))
}

/// Match one normalized subject against a pattern for its rule class.
///
/// Path matching here is **case-sensitive** on every platform. Windows write/edit
/// **deny** identity folding is applied in [`evaluate_permission_rules`], not
/// in this primitive — allow and deny path semantics deliberately differ.
pub fn permission_rule_matches(prefix: PermissionRulePrefix, pattern: &str, subject: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }

    match prefix {
        PermissionRulePrefix::Process => process_rule_matches(pattern, subject),
        PermissionRulePrefix::Write | PermissionRulePrefix::Edit => {
            wildcard_match(pattern, subject, WildcardMode::PortablePath)
        }
    }
}

fn process_rule_matches(pattern: &str, subject: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let Some(tokens) = decode_process_subject(subject) else {
        // Undecodable / legacy flat subjects never match non-empty patterns.
        return false;
    };

    // When argv is bijective under space-join, keep the historical Grok-style
    // character prefix/glob surface so patterns like `cargo *` and `node*` still
    // work for ordinary toolchain commands.
    if let Some(flat) = bijective_space_join(&tokens) {
        return flat.starts_with(pattern) || wildcard_match(pattern, &flat, WildcardMode::Process);
    }

    // Non-bijective argv (embedded spaces, empty args, …): match only against
    // the injective encoding. Space-oriented patterns cannot alias a different
    // structured argv. Broad program patterns (`node*`) still hit the encoded
    // subject via character prefix/glob on the canonical form.
    subject.starts_with(pattern) || wildcard_match(pattern, subject, WildcardMode::Process)
}

/// Whether write/edit **deny** matching should use OS path-identity case fold.
///
/// Windows: yes (NTFS default case-insensitive). Other platforms: no (byte-wise
/// case-sensitive deny, matching typical Unix FS semantics).
pub fn path_deny_uses_identity_case_fold() -> bool {
    cfg!(windows)
}

fn fold_path_identity(path: &str) -> String {
    path.to_lowercase()
}

fn path_deny_matches(pattern: &str, subject: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if path_deny_uses_identity_case_fold() {
        let pattern = fold_path_identity(pattern);
        let subject = fold_path_identity(subject);
        wildcard_match(&pattern, &subject, WildcardMode::PortablePath)
    } else {
        wildcard_match(pattern, subject, WildcardMode::PortablePath)
    }
}

/// Evaluate deny before allow. Input order breaks ties within the same action.
/// A project-scoped allow is ignored even if a corrupt caller supplies one.
///
/// Process **allow** is gated by [`process_subject_confirmed_for_allow`]: a
/// matching allow only auto-approves when the subject shape can be confirmed
/// free of opaque evaluation / command-delegation payloads. Anything we cannot
/// confirm fails closed to [`PermissionRuleEvaluation::RequireConfirmation`].
/// Deny is never weakened by this gate.
///
/// Write/edit **deny** uses platform path-identity folding (see module docs).
/// Write/edit **allow** stays case-sensitive on every platform.
pub fn evaluate_permission_rules(
    prefix: PermissionRulePrefix,
    subject: &str,
    rules: &[PermissionRuleSpec],
) -> PermissionRuleEvaluation {
    let mut first_deny = None;
    let mut first_allow = None;

    for rule in rules {
        if rule.prefix != prefix
            || (rule.scope == PermissionRuleScope::Project
                && rule.action == PermissionRuleAction::Allow)
        {
            continue;
        }

        let hits = match (prefix, rule.action) {
            (
                PermissionRulePrefix::Write | PermissionRulePrefix::Edit,
                PermissionRuleAction::Deny,
            ) => path_deny_matches(&rule.pattern, subject),
            _ => permission_rule_matches(prefix, &rule.pattern, subject),
        };
        if !hits {
            continue;
        }

        let matched = MatchedPermissionRule {
            rule: rule.clone(),
            match_subject: subject.to_owned(),
        };
        match rule.action {
            PermissionRuleAction::Deny if first_deny.is_none() => first_deny = Some(matched),
            PermissionRuleAction::Allow if first_allow.is_none() => first_allow = Some(matched),
            PermissionRuleAction::Deny | PermissionRuleAction::Allow => {}
        }
    }

    if let Some(matched) = first_deny {
        PermissionRuleEvaluation::Deny(matched)
    } else if let Some(matched) = first_allow {
        if prefix == PermissionRulePrefix::Process && !process_subject_confirmed_for_allow(subject)
        {
            PermissionRuleEvaluation::RequireConfirmation
        } else {
            PermissionRuleEvaluation::Allow(matched)
        }
    } else {
        PermissionRuleEvaluation::RequireConfirmation
    }
}

/// Whether a process subject is confirmed free of opaque evaluation and
/// command-delegation payloads for allow auto-approval.
///
/// Failure direction is **allowlist of confirmed-safe shapes**, not a denylist
/// of interpreters: unknown programs that introduce code/command payloads are
/// unconfirmed and must prompt. Programs that reuse common short flags for
/// non-eval meanings (e.g. `git -c key=value`, `grep -e pattern`, `ls -c`,
/// `ps -e`) are positively confirmed for those flag letters only.
///
/// Shape analysis runs on the **decoded** program/argv token list. Legacy
/// space-joined subjects that do not decode return `false` (fail-closed).
pub fn process_subject_confirmed_for_allow(subject: &str) -> bool {
    let Some(tokens) = decode_process_subject(subject) else {
        return false;
    };
    if tokens.is_empty() {
        return false;
    }

    let program = program_basename(&tokens[0]);
    if command_delegation_program(program) {
        // Outer subject alone cannot confirm what the delegated command is.
        return false;
    }

    let mut index = 1usize;
    while index < tokens.len() {
        let token = tokens[index].as_str();

        if matches!(token, "-exec" | "-execdir") {
            return false;
        }

        if token == "eval" {
            // Subcommand form (`deno eval …`) is an opaque code payload.
            return false;
        }

        if let Some(rest) = token.strip_prefix("--") {
            if rest.is_empty() {
                // `--` end-of-options: remaining tokens are operands, not flags.
                return true;
            }
            let name = rest.split_once('=').map_or(rest, |(name, _)| name);
            if long_option_introduces_code_payload(name) {
                return false;
            }
            index += 1;
            continue;
        }

        if token.starts_with('-') && token != "-" {
            let flags = &token[1..];
            if flags.is_empty() {
                index += 1;
                continue;
            }
            // Only single-letter short options and short clusters (`-e`, `-xc`).
            // Multi-character single-dash words (`-name`, `-type`, `-module`) are
            // not short clusters; `-exec` is handled by the exact match above.
            if is_short_flag_cluster(flags)
                && short_flags_introduce_unconfirmed_payload(program, flags)
            {
                return false;
            }
            index += 1;
            continue;
        }

        index += 1;
    }

    true
}

/// Whether a process **allow** pattern has no argument constraint.
///
/// Overbroad means a later, semantically different argv of the same program
/// (or a glued program-name glob) would match. Deny patterns are out of
/// scope: this classifier is only consulted when *producing* or *warning
/// about* allow rules.
///
/// Exact v1-encoded subjects pin one argv and are never overbroad.
pub fn is_overbroad_process_allow_pattern(pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if pattern.contains('\u{1e}') {
        // Decodable v1 form is one argv. Undecodable junk is treated as
        // overbroad so we never hide a warning behind a malformed pattern.
        return decode_process_subject(pattern).is_none();
    }

    let tokens: Vec<&str> = pattern
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return true;
    }
    if program_token_is_unconstrained(tokens[0]) {
        return true;
    }
    !tokens
        .iter()
        .skip(1)
        .any(|token| !token_is_only_wildcards(token))
}

fn program_token_is_unconstrained(program: &str) -> bool {
    program.is_empty() || program.contains('*')
}

fn token_is_only_wildcards(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|ch| ch == '*')
}

/// Suggest a durable **allow** pattern from a process subject.
///
/// Unlike the historical renderer default (`program *` / `program first-arg *`),
/// this keeps every concrete argv token so approving `node build.js` cannot
/// casually persist a rule that auto-allows `node anything-else.js`.
///
/// Bare programs still produce an overbroad pattern (`node`) because prefix
/// match has no narrower space-join form. Callers must warn; they must not
/// rewrite already-stored rules.
pub fn suggest_process_allow_pattern(subject: &str) -> String {
    let Some(tokens) = decode_process_subject(subject) else {
        return subject.to_owned();
    };
    if tokens.is_empty() || bijective_space_join(&tokens).is_none() {
        return subject.to_owned();
    }
    tokens.join(" ")
}

/// A different, confirmable process subject that `pattern` would auto-allow,
/// derived from `from_subject`. `None` when the pattern is not overbroad or
/// no real auto-allow contrast can be produced.
pub fn example_also_allowed_process_subject(pattern: &str, from_subject: &str) -> Option<String> {
    if !is_overbroad_process_allow_pattern(pattern) {
        return None;
    }
    let tokens = decode_process_subject(from_subject)?;
    if tokens.is_empty() {
        return None;
    }
    let contrast = contrast_process_argv(&tokens);
    let subject = canonical_process_subject(&contrast[0], &contrast[1..]);
    if subject == from_subject {
        return None;
    }
    if !process_subject_confirmed_for_allow(&subject) {
        return None;
    }
    if !permission_rule_matches(PermissionRulePrefix::Process, pattern, &subject) {
        return None;
    }

    let probe = PermissionRuleSpec {
        rule_id: "overbroad-contrast".to_owned(),
        scope: PermissionRuleScope::Global,
        project_id: None,
        prefix: PermissionRulePrefix::Process,
        pattern: pattern.to_owned(),
        action: PermissionRuleAction::Allow,
        content_digest: "sha256:overbroad-contrast".to_owned(),
    };
    match evaluate_permission_rules(PermissionRulePrefix::Process, &subject, &[probe]) {
        PermissionRuleEvaluation::Allow(_) => Some(subject),
        PermissionRuleEvaluation::Deny(_) | PermissionRuleEvaluation::RequireConfirmation => None,
    }
}

/// Derive a sibling argv from the approved tokens. The first operand is
/// rewritten by appending `.other` so the example is mechanical, not invented.
fn contrast_process_argv(tokens: &[String]) -> Vec<String> {
    let mut contrast = tokens.to_vec();
    if contrast.len() == 1 {
        let marker = format!("{}.other", contrast[0]);
        contrast.push(marker);
    } else if contrast[1].starts_with('-') {
        let marker = format!("{}.other", contrast[0]);
        contrast.insert(1, marker);
    } else {
        contrast[1] = format!("{}.other", contrast[1]);
    }
    contrast
}

fn program_basename(program: &str) -> &str {
    let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
    if base.len() > 4 && base[base.len() - 4..].eq_ignore_ascii_case(".exe") {
        &base[..base.len() - 4]
    } else {
        base
    }
}

fn command_delegation_program(program: &str) -> bool {
    let program = program.to_ascii_lowercase();
    // These programs exist primarily to launch another command or package.
    // The outer subject cannot confirm the delegated payload is safe.
    matches!(
        program.as_str(),
        "xargs" | "npx" | "pnpx" | "bunx" | "uvx" | "env"
    )
}

fn long_option_introduces_code_payload(name: &str) -> bool {
    matches!(
        name,
        "eval" | "print" | "command" | "execute" | "expression"
    )
}

/// True for single-letter short options and compact clusters (`-e`, `-ef`,
/// `-xc`). Longer single-dash tokens are treated as multi-character options
/// (find `-name`, etc.), not as clustered short flags.
fn is_short_flag_cluster(flags: &str) -> bool {
    let len = flags.chars().count();
    (1..=3).contains(&len) && flags.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// Short-flag clusters that introduce an unconfirmed code/command payload
/// unless this program positively reuses the letter for a non-eval meaning.
fn short_flags_introduce_unconfirmed_payload(program: &str, flags: &str) -> bool {
    let program = program.to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(program.as_str());

    for flag in flags.chars() {
        match flag {
            'e' if !program_confirms_non_eval_e(program) => return true,
            'c' if !program_confirms_non_eval_c(program) => return true,
            // PHP and some hosts use `-r` for raw/eval code.
            'r' if !program_confirms_non_eval_r(program) => return true,
            _ => {}
        }
    }
    false
}

fn program_confirms_non_eval_e(program: &str) -> bool {
    // These tools use `-e` for pattern/select semantics, not arbitrary code.
    matches!(
        program,
        "grep"
            | "egrep"
            | "fgrep"
            | "rg"
            | "ag"
            | "ps"
            | "git"
            | "ls"
            | "dir"
            | "echo"
            | "printf"
            | "head"
            | "tail"
            | "sort"
            | "uniq"
            | "wc"
            | "cut"
            | "tr"
            | "cat"
            | "chmod"
            | "chown"
            | "date"
            | "tar"
            | "rsync"
            | "ssh"
            | "scp"
            | "curl"
            | "wget"
            | "cargo"
            | "rustc"
            | "npm"
            | "pnpm"
            | "yarn"
            | "pip"
            | "pip3"
            | "git-lfs"
    )
}

fn program_confirms_non_eval_c(program: &str) -> bool {
    // `-c` as config/cipher/sort/count — not script body — for these tools.
    matches!(
        program,
        "git"
            | "ssh"
            | "scp"
            | "ls"
            | "dir"
            | "ps"
            | "wc"
            | "curl"
            | "wget"
            | "tar"
            | "rsync"
            | "chmod"
            | "chown"
            | "cargo"
            | "rustc"
            | "npm"
            | "pnpm"
            | "yarn"
            | "pip"
            | "pip3"
            | "head"
            | "tail"
            | "sort"
            | "uniq"
            | "cut"
            | "tr"
            | "cat"
            | "echo"
            | "printf"
            | "date"
    )
}

fn program_confirms_non_eval_r(program: &str) -> bool {
    // `-r` is recursive/reverse for many tools; only leave interpreters unconfirmed.
    matches!(
        program,
        "rm" | "cp"
            | "mv"
            | "ls"
            | "dir"
            | "chmod"
            | "chown"
            | "grep"
            | "rg"
            | "git"
            | "ssh"
            | "scp"
            | "rsync"
            | "tar"
            | "sort"
            | "ps"
            | "cargo"
            | "rustc"
            | "npm"
            | "pnpm"
            | "yarn"
            | "pip"
            | "pip3"
            | "curl"
            | "wget"
            | "head"
            | "tail"
            | "cat"
            | "echo"
            | "printf"
            | "date"
            | "wc"
            | "cut"
            | "tr"
            | "uniq"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WildcardMode {
    Process,
    PortablePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(char),
    AnySequence {
        crosses_slash: bool,
    },
    /// Path-only `**/`: zero or more complete directory segments.
    AnyDirectories,
}

fn wildcard_match(pattern: &str, subject: &str, mode: WildcardMode) -> bool {
    let tokens = tokenize(pattern, mode);
    let subject = subject.chars().collect::<Vec<_>>();
    let mut current = vec![false; subject.len() + 1];
    current[0] = true;

    for token in tokens {
        let mut next = vec![false; subject.len() + 1];
        match token {
            Token::Literal(expected) => {
                for index in 0..subject.len() {
                    if current[index] && subject[index] == expected {
                        next[index + 1] = true;
                    }
                }
            }
            Token::AnySequence { crosses_slash } => {
                next[0] = current[0];
                for end in 1..=subject.len() {
                    let can_extend = next[end - 1] && (crosses_slash || subject[end - 1] != '/');
                    next[end] = current[end] || can_extend;
                }
            }
            Token::AnyDirectories => {
                next.clone_from_slice(&current);
                let mut has_start = false;
                for end in 1..=subject.len() {
                    has_start |= current[end - 1];
                    if has_start && subject[end - 1] == '/' {
                        next[end] = true;
                    }
                }
            }
        }
        current = next;
    }

    current[subject.len()]
}

fn tokenize(pattern: &str, mode: WildcardMode) -> Vec<Token> {
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut tokens = Vec::with_capacity(chars.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '*' {
            tokens.push(Token::Literal(chars[index]));
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && chars[index] == '*' {
            index += 1;
        }
        let star_count = index - start;

        match mode {
            WildcardMode::Process => tokens.push(Token::AnySequence {
                crosses_slash: true,
            }),
            WildcardMode::PortablePath if star_count >= 2 && chars.get(index) == Some(&'/') => {
                tokens.push(Token::AnyDirectories);
                index += 1;
            }
            WildcardMode::PortablePath => tokens.push(Token::AnySequence {
                crosses_slash: star_count >= 2,
            }),
        }
    }

    tokens
}
