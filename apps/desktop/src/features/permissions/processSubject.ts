/**
 * Renderer mirror of the host process-subject v1 encoding.
 *
 * The host (`keeper-policy::canonical_process_subject`) encodes a process rule
 * subject so argv boundaries survive the round trip. A space-joined subject is
 * not injective — `["safe.js", "--version"]` and `["safe.js --version"]` share
 * one string — which let an `allow node safe.js *` rule approve a semantically
 * different command (external audit EA-02).
 *
 * The renderer needs the same encoding for two reasons, and only these two:
 *
 * 1. **Pattern suggestion.** A durable rule is stored as a pattern string. The
 *    suggestion must be built from real argv tokens, never from splitting the
 *    encoded subject on whitespace — the encoding contains spaces and a record
 *    separator that whitespace splitting would mangle into a rule that can
 *    never match anything.
 * 2. **Display.** A raw v1 subject carries a control character and hex length
 *    prefixes. Showing that to a human reviewing an approval is unreadable.
 *
 * The renderer never *decides* anything from these values. Matching lives in
 * the host; this module only shapes what the user reads and edits.
 *
 * Both sides are pinned to one shared vector file — see
 * `tests/fixtures/process-subject-v1.json` and the conformance test on each
 * side. If either implementation drifts, one of those two tests goes red.
 */

/**
 * ASCII record separator (U+001E). Built from its code point on purpose: a
 * literal control character in source is invisible to review and easily lost
 * in transit through editors, patches and terminals.
 */
const RECORD_SEPARATOR = String.fromCharCode(0x1e);
/** Mirrors the host's `PROCESS_SUBJECT_V1_MARK`. */
const V1_MARK = `${RECORD_SEPARATOR}p1`;
const COLON = 0x3a;
/**
 * A length prefix wider than a 64-bit `usize` in hex cannot parse host-side and
 * returns `None` there. Bounding the scan keeps the two sides in step and stops
 * a malformed subject from driving an unbounded string build here.
 */
const MAX_LENGTH_PREFIX_DIGITS = 16;

const utf8Encoder = new TextEncoder();
/** `fatal` so a length prefix that splits a character fails instead of replacing. */
const utf8Decoder = new TextDecoder("utf-8", { fatal: true });

/**
 * Encode `(program, argv)` the way the host does.
 *
 * Empty argv encodes to the bare program so durable receipt validation
 * (`match_subject == program`) still holds.
 */
export function encodeProcessSubject(
  program: string,
  argv: readonly string[],
): string {
  if (argv.length === 0) return program;
  let subject = `${program} ${V1_MARK}`;
  for (const argument of argv) {
    const byteLength = utf8Encoder.encode(argument).length;
    subject += `:${byteLength.toString(16)}:${argument}`;
  }
  return subject;
}

/** Read an ASCII hex run without spreading an unbounded array onto the stack. */
function readLengthPrefix(bytes: Uint8Array, end: number): number | null {
  if (end <= 0 || end > MAX_LENGTH_PREFIX_DIGITS) return null;
  let hex = "";
  for (let index = 0; index < end; index += 1) {
    hex += String.fromCharCode(bytes[index]);
  }
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null;
  // Reject leading zeros except the single digit `0` (empty argument).
  if (hex.length > 1 && hex.startsWith("0")) return null;
  return Number.parseInt(hex, 16);
}

/**
 * Decode a v1 subject into `[program, ...argv]`.
 *
 * Returns `null` for legacy space-joined subjects and any malformed payload.
 * Callers must treat `null` as "the host cannot match this" — never as
 * "assume it is a plain command line".
 */
export function decodeProcessSubject(subject: string): string[] | null {
  if (subject.length === 0 || subject.includes("\0")) return null;

  // Program-only form (empty argv).
  if (!subject.includes(" ")) {
    if (subject.includes(RECORD_SEPARATOR)) return null;
    return [subject];
  }

  const spaceAt = subject.indexOf(" ");
  const program = subject.slice(0, spaceAt);
  const afterProgram = subject.slice(spaceAt + 1);
  if (program.length === 0 || program.includes(RECORD_SEPARATOR)) return null;
  if (!afterProgram.startsWith(V1_MARK)) {
    // Legacy flat "program arg1 arg2" subjects intentionally do not decode.
    return null;
  }

  // Length prefixes count UTF-8 bytes, so walk bytes rather than UTF-16 units.
  let cursor = utf8Encoder.encode(afterProgram.slice(V1_MARK.length));
  // A mark with no arguments is not a produced form.
  if (cursor.length === 0) return null;

  const tokens = [program];
  while (cursor.length > 0) {
    if (cursor[0] !== COLON) return null;
    cursor = cursor.subarray(1);

    const separator = cursor.indexOf(COLON);
    if (separator < 0) return null;
    const length = readLengthPrefix(cursor, separator);
    if (length === null) return null;

    const payload = cursor.subarray(separator + 1);
    if (payload.length < length) return null;
    let argument: string;
    try {
      argument = utf8Decoder.decode(payload.subarray(0, length));
    } catch {
      return null;
    }
    tokens.push(argument);
    cursor = payload.subarray(length);
  }
  return tokens;
}

/**
 * Whether space-joining these tokens is reversible.
 *
 * Mirrors the host's `bijective_space_join`. Only when this holds may a
 * space-oriented pattern (`cargo test *`) be suggested — otherwise the pattern
 * would describe a different argv than the one the user is approving.
 */
export function spaceJoinIsBijective(tokens: readonly string[]): boolean {
  if (tokens.length === 0) return false;
  return !tokens.some(
    (token) =>
      token.length === 0
      || token.includes(" ")
      || token.includes(RECORD_SEPARATOR),
  );
}

/** Quote an argument only when a bare rendering would misread its boundaries. */
function quoteForDisplay(argument: string): string {
  const needsQuotes = argument.length === 0
    || argument.includes(" ")
    || argument.includes("\t")
    || argument.includes('"')
    || argument.includes(RECORD_SEPARATOR);
  if (!needsQuotes) return argument;
  return `"${argument.replace(/"/g, '\\"')}"`;
}

/**
 * Human-readable rendering of a host subject, for approval and rule surfaces.
 *
 * Display only. It is deliberately *not* reversible — never parse it back into
 * a rule. Undecodable subjects are shown verbatim rather than guessed at, so a
 * legacy value looks like what it is.
 */
export function formatProcessSubjectForDisplay(subject: string): string {
  const tokens = decodeProcessSubject(subject);
  if (!tokens || tokens.length === 0) return subject;
  const [program, ...argv] = tokens;
  if (argv.length === 0) return program;
  return [program, ...argv.map(quoteForDisplay)].join(" ");
}

/**
 * True when the subject carries the v1 encoding, i.e. its stored form differs
 * from its display form. Surfaces use this to decide whether the user needs to
 * be shown both.
 */
export function processSubjectIsEncoded(subject: string): boolean {
  return subject.includes(RECORD_SEPARATOR);
}
