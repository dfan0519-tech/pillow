# Detect + Catalog Host — execution report

**Status:** complete  
**Worker:** grok1 (write)  
**Controller:** Claude  
**Date:** 2026-08-12  

## Completion signal

`DETECT-CATALOG-HOST-COMPLETE`

---

## 0. Boundary

| Item | Rule / outcome |
| --- | --- |
| Primary allowlist | `apps/desktop/src-tauri/src/**`, `apps/desktop/src/features/settings/**`, `apps/desktop/tests/**`, this report |
| Forbidden | root `Cargo.toml` (untouched), git write ops (none), gates/E2E (not run) |
| No secrets in report | no absolute machine paths, no keys/tokens |

### Required wiring outside primary allowlist

| Path | Why | Action taken |
| --- | --- | --- |
| `apps/desktop/src-tauri/Cargo.toml` | host must depend on workspace crate `pillow-cli-detect` | path dependency added |
| `crates/runtime-slots` | draft probe must call existing `fetch_openai_model_catalog` without a second HTTP client (was `pub(crate)`) | re-export as `pub` only |

No new external crates. No root workspace member changes. Renderer still has **zero** outbound `fetch`/`XMLHttpRequest` for model catalogs.

---

## 1. Exit codes (real runs)

Commands (no pipe to `tail`; full log files inspected):

| Command | Exit | Observed |
| --- | ---: | --- |
| `corepack pnpm typecheck` | **0** | `@pillow/desktop` `tsc --noEmit` green |
| `corepack pnpm --filter @pillow/desktop test` | **0** | Vitest **46** files, **559** tests passed |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | **0** | **184** passed; **0** failed; **2** ignored |

---

## 2. T1 DTO keys + renderer allowlist (same batch)

### Host `LocalCliDetectionDto` (camelCase wire)

| Key | Type / notes |
| --- | --- |
| `kind` | `grok` \| `claude` \| `codex` \| `opencode` \| `gemini` |
| `found` | boolean |
| `executablePath` | string \| null |
| `version` | string \| null |
| `authStatus` | `authenticated` \| `not_authenticated` \| `unknown` |
| `accountLabel` | string \| null (authenticated only; sanitized) |
| `authReason` | string \| null (`unknown` reason codes) |

Command: `detect_local_clis` — registered in `lib.rs` `generate_handler!`.  
Per-CLI hard timeout: **8s** via `detect_all_with_timeout`.

### Renderer closed allowlist (same change set)

`LOCAL_CLI_DETECTION_DTO_KEYS` in `runtimeSlotsGateway.ts` matches the seven keys above.  
Parse: `parseLocalCliDetectionView` + `assertNoSecretPayloadFields`.

### Host tests pinning closed keys

- `local_cli_detection_dto_keys_are_closed_and_secret_free`
- `detect_local_clis_returns_all_kinds_with_closed_dto` (five kinds, closed JSON)
- `every_command_the_slots_gateway_invokes_is_registered` (gateway source → handler)

---

## 3. `unknown` ≠ 未登录 (positive + negative)

| Layer | Test | Assertion |
| --- | --- | --- |
| Host map | `local_cli_auth_unknown_is_never_not_authenticated` | `NotAuthenticated` → `not_authenticated`; `Unknown{timed_out\|not_found\|no_headless_status_command\|…}` → `unknown` + reason, **never** `not_authenticated` |
| Gateway label | `formatLocalCliAuthLabel — unknown ≠ 未登录` | unknown → **未确定** (+ reason); contains no **未登录** |
| Gateway label | positive not_authenticated | exact **未登录** |
| UI | `scan lists results; select fills path; unknown is 未确定 not 未登录` | gemini row `data-auth-status` copy is 未确定 + reason; claude is 未登录; select codex fills path + `login status` |

UI helper: `formatLocalCliAuthLabel` — fail-closed: any non-authenticated/non-not_authenticated value → 未确定.

---

## 4. T3 success / unsupported / fail

Host command: `probe_model_catalog(base_url, api_key)` → `ProbeModelCatalogDto`  
Reuses `pillow_runtime_slots::fetch_openai_model_catalog` (WinHTTP + same 404/405/501 → unsupported semantics).

| Path | Host test | Renderer test |
| --- | --- | --- |
| **Success** | `probe_model_catalog_success_returns_models_without_echoing_key` (Windows TCP) — models listed, status `available` | UI: draft select lists models; choosing fills model id |
| **Unsupported** | `probe_model_catalog_unsupported_endpoint_is_normal_result` (HTTP 404) — `status=unsupported`, `reasonCode=endpoint_not_supported`, empty models | UI: `runtime-slots-draft-models-unsupported`; no select; model id stays empty |
| **Fail** | `probe_model_catalog_failure_does_not_pretend_success_or_echo_key` (HTTP 401) — `Err(runtime_slot_model_catalog_unavailable)` | UI: error alert; no select; model id empty |

DTO keys: `status`, `source`, `models`, `reasonCode` — `PROBE_MODEL_CATALOG_DTO_KEYS` allowlist same batch.

Empty key/base_url → `runtime_slot_input_invalid` (no network).

---

## 5. Secret never in outputs

| Evidence | What is pinned |
| --- | --- |
| Host success DTO | `serde_json` of probe result must not contain the marker key |
| Host fail | `CommandError.message` / `.code` must not contain the marker |
| Host blank key | whitespace key → invalid input; no network |
| Gateway parse | `assertNoSecretPayloadFields` rejects smuggled `apiKey` on probe/detect DTOs |
| Gateway invoke fail | host message with `apiKey=…` is **not** echoed into gateway error text |
| Mock | `lastProbeModelCatalogCalls` stores **length only**, never key text |
| UI | error/unsupported status text must not contain draft key; password field may still hold draft until save (by design) |

`api_key` is command **input only** — moved into `SecretBytes` immediately in `probe_model_catalog_impl`.

---

## 6. Write boundary audit + residual risk

### Files touched

| Area | Paths |
| --- | --- |
| Host commands | `runtime_slot_commands.rs`, `runtime_slot_commands_tests.rs`, `lib.rs` |
| Desktop dep | `apps/desktop/src-tauri/Cargo.toml` (+ `pillow-cli-detect`) |
| Catalog re-export | `crates/runtime-slots/src/catalog.rs`, `lib.rs` (`pub fn fetch_openai_model_catalog`) |
| Renderer | `runtimeSlotsTypes.ts`, `runtimeSlotsGateway.ts`, `RuntimeSlotsSection.tsx` |
| Tests | `detect-local-clis.test.ts`, `probe-model-catalog.test.ts`, `runtime-slots-detect-catalog-ui.test.tsx` |
| Report | this file |

### Product behaviour

1. **Create 本机 CLI:**「扫描本机 CLI」→ host detect → pick row fills path + default status argv; manual path retained. Auth three-state shown honestly.
2. **Create/edit API key:**「拉取模型列表」→ host `probe_model_catalog` only; unsupported/fail leave model id empty (no invent).
3. **Local CLI still cannot be 总控** — existing tests untouched and still green.

### Residual risk

| Risk | Notes |
| --- | --- |
| Detect latency | Up to ~8s × 5 CLIs when none respond; UI shows busy copy. |
| Live detect in unit test | `detect_local_clis_returns_all_kinds_with_closed_dto` probes the machine (machine-dependent `found`). |
| `crates/runtime-slots` pub surface | One function re-exported for draft reuse; other Worker work on crates should keep this `pub`. |
| Cargo.lock | Path dep may refresh lock on first build (not hand-edited here). |
| No E2E / full gate | Controller-owned; this report only claims the three listed exit codes. |

---

## Deliverables map

| Task | Delivery |
| --- | --- |
| T1 | `detect_local_clis` + closed DTO + gateway allowlist + handler registration |
| T2 | Create-form detect list/select + 未确定 UI + manual path |
| T3 | `probe_model_catalog` host path reusing catalog HTTP; draft UI pull button |
