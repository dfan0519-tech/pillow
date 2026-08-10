/**
 * Sanitize process-captured text so harness records stay valid JSON and
 * evidence of odd bytes is preserved (never silently deleted).
 *
 * Primary failure mode this guards against: PowerShell ConvertTo-Json (and
 * similar producers) can emit raw U+0000–U+001F inside string literals.
 * JSON.parse then throws "Bad control character in string literal…", which
 * turns a green product run into a harness false-red.
 */

const CONTROL_CHAR_RE = /[\u0000-\u001F]/gu;

/**
 * Replace U+0000–U+001F with visible `<0xNN>` placeholders.
 * Does not delete characters; records how many replacements occurred.
 *
 * @param {unknown} value
 * @returns {{ text: string, originalLength: number, replacedCount: number }}
 */
export function sanitizeControlCharsVisible(value) {
  const text = String(value ?? "");
  let replacedCount = 0;
  const sanitized = text.replace(CONTROL_CHAR_RE, (ch) => {
    replacedCount += 1;
    const hex = ch.charCodeAt(0).toString(16).padStart(2, "0");
    return `<0x${hex}>`;
  });
  return {
    text: sanitized,
    originalLength: text.length,
    replacedCount,
  };
}

/**
 * Escape raw control characters that appear inside JSON string literals so
 * producer-buggy JSON can be parsed. Outside strings, tab/LF/CR are kept;
 * other control characters become a single space (counted).
 * Inside strings, controls become `\u00XX` (content preserved after parse).
 *
 * @param {unknown} raw
 * @returns {{ text: string, originalLength: number, controlCharsEscaped: number }}
 */
export function escapeRawControlsInJsonText(raw) {
  const text = String(raw ?? "");
  let out = "";
  let inString = false;
  let escaped = false;
  let controlCharsEscaped = 0;

  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    const code = ch.charCodeAt(0);

    if (inString) {
      if (escaped) {
        out += ch;
        escaped = false;
        continue;
      }
      if (ch === "\\") {
        out += ch;
        escaped = true;
        continue;
      }
      if (ch === "\"") {
        out += ch;
        inString = false;
        continue;
      }
      if (code <= 0x1f) {
        out += `\\u${code.toString(16).padStart(4, "0")}`;
        controlCharsEscaped += 1;
        continue;
      }
      out += ch;
      continue;
    }

    if (ch === "\"") {
      out += ch;
      inString = true;
      continue;
    }
    if (code <= 0x1f && ch !== "\t" && ch !== "\n" && ch !== "\r") {
      out += " ";
      controlCharsEscaped += 1;
      continue;
    }
    out += ch;
  }

  return {
    text: out,
    originalLength: text.length,
    controlCharsEscaped,
  };
}

/**
 * Parse JSON captured from an external process. Always runs control-char
 * escape on the raw text *before* parse (sanitize-at-capture), so a real
 * structural JSON error still throws with evidence intact.
 *
 * @param {unknown} raw
 * @returns {{ value: unknown, originalLength: number, controlCharsEscaped: number }}
 */
export function parseJsonFromCapturedText(raw) {
  const source = String(raw ?? "");
  const { text, originalLength, controlCharsEscaped } =
    escapeRawControlsInJsonText(source);
  const value = JSON.parse(text);
  return { value, originalLength, controlCharsEscaped };
}

/**
 * Format a process stream for embedding into harness error / record text.
 * Visible placeholders + length metadata; never silent deletion.
 *
 * @param {string} label
 * @param {unknown} raw
 * @returns {string}
 */
export function formatCapturedStream(label, raw) {
  const { text, originalLength, replacedCount } =
    sanitizeControlCharsVisible(raw);
  const meta = replacedCount > 0
    ? `[${label}: originalLength=${originalLength}, controlCharsReplaced=${replacedCount}]\n`
    : `[${label}: originalLength=${originalLength}]\n`;
  return `${meta}${text}`;
}

function selfTest() {
  const ctrl = String.fromCharCode(3);
  const bell = String.fromCharCode(7);
  const rawJson = `{"commandLine":"foo${ctrl}bar${bell}baz","pid":1}`;

  // Before: raw control chars inside a JSON string literal → parse throws.
  let threw = false;
  try {
    JSON.parse(rawJson);
  } catch (error) {
    threw = /control character/i.test(
      error instanceof Error ? error.message : String(error),
    );
  }
  if (!threw) {
    throw new Error("self-test expected JSON.parse of raw controls to throw");
  }

  // After: sanitize-at-capture then parse succeeds; content recoverable.
  const { value, originalLength, controlCharsEscaped } =
    parseJsonFromCapturedText(rawJson);
  if (controlCharsEscaped !== 2) {
    throw new Error(
      `self-test expected 2 escapes, got ${controlCharsEscaped}`,
    );
  }
  if (originalLength !== rawJson.length) {
    throw new Error("self-test originalLength mismatch");
  }
  if (
    !value
    || typeof value !== "object"
    || /** @type {{ commandLine?: string }} */ (value).commandLine
      !== `foo${ctrl}bar${bell}baz`
  ) {
    throw new Error("self-test parsed commandLine mismatch");
  }

  const visible = sanitizeControlCharsVisible(`x${ctrl}y`);
  if (visible.text !== "x<0x03>y" || visible.replacedCount !== 1) {
    throw new Error(`self-test visible sanitize failed: ${visible.text}`);
  }

  const stream = formatCapturedStream("stdout", `hi${ctrl}`);
  if (!stream.includes("controlCharsReplaced=1") || !stream.includes("<0x03>")) {
    throw new Error(`self-test stream format failed: ${stream}`);
  }

  // Structural errors must still throw (no swallow).
  let structuralThrew = false;
  try {
    parseJsonFromCapturedText("{not-json");
  } catch {
    structuralThrew = true;
  }
  if (!structuralThrew) {
    throw new Error("self-test expected structural JSON error to throw");
  }

  // Valid JSON without controls is a no-op path.
  const clean = parseJsonFromCapturedText('{"ok":true}');
  if (clean.controlCharsEscaped !== 0 || clean.value?.ok !== true) {
    throw new Error("self-test clean JSON path failed");
  }

  // Embedding sanitized streams into a record must produce valid JSON.
  const record = {
    status: "failed",
    error: formatCapturedStream("stdout", `Bad\x00stuff${ctrl}`),
  };
  const encoded = JSON.stringify(record);
  const decoded = JSON.parse(encoded);
  if (!decoded.error.includes("<0x00>") || !decoded.error.includes("<0x03>")) {
    throw new Error("self-test record round-trip lost placeholders");
  }

  console.log(
    `JSON_TEXT_SANITIZE_SELF_TEST ${JSON.stringify({
      status: "passed",
      controlCharsEscaped,
      visibleReplaced: visible.replacedCount,
    })}`,
  );
}

// Runnable as: node scripts/lib/json-text-sanitize.mjs --self-test
if (process.argv[1] && process.argv.includes("--self-test")) {
  const entry = process.argv[1].replaceAll("\\", "/");
  if (entry.endsWith("/json-text-sanitize.mjs") || entry.endsWith("json-text-sanitize.mjs")) {
    selfTest();
  }
}
