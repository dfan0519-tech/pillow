//! Pure permission-rule matching and precedence.
//!
//! Pillow pins a small deterministic wildcard language rather than claiming
//! bit-for-bit Grok compatibility. Matching is case-sensitive on every host;
//! only `*` and `**` are operators. Process rules use literal-prefix OR
//! whole-subject wildcard matching. Write/edit rules use whole portable-path
//! wildcard matching only.

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
    segments.push(tokens.join(" "));
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

/// Canonical process subject: catalog label followed by literal argv tokens,
/// each separated by one ASCII space. No shell quoting or parsing occurs.
pub fn canonical_process_subject(program: &str, argv: &[String]) -> String {
    let suffix_bytes = argv.iter().map(|arg| arg.len() + 1).sum::<usize>();
    let mut subject = String::with_capacity(program.len() + suffix_bytes);
    subject.push_str(program);
    for argument in argv {
        subject.push(' ');
        subject.push_str(argument);
    }
    subject
}

/// Match one normalized subject against a pattern for its rule class.
pub fn permission_rule_matches(prefix: PermissionRulePrefix, pattern: &str, subject: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }

    match prefix {
        PermissionRulePrefix::Process => {
            subject.starts_with(pattern) || wildcard_match(pattern, subject, WildcardMode::Process)
        }
        PermissionRulePrefix::Write | PermissionRulePrefix::Edit => {
            wildcard_match(pattern, subject, WildcardMode::PortablePath)
        }
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
            || !permission_rule_matches(prefix, &rule.pattern, subject)
        {
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
/// This is pure shape analysis on the already-canonical space-joined subject.
pub fn process_subject_confirmed_for_allow(subject: &str) -> bool {
    let tokens: Vec<&str> = subject
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return false;
    }

    let program = program_basename(tokens[0]);
    if command_delegation_program(program) {
        // Outer subject alone cannot confirm what the delegated command is.
        return false;
    }

    let mut index = 1usize;
    while index < tokens.len() {
        let token = tokens[index];

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
