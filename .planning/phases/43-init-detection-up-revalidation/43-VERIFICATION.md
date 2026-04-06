---
phase: 43-init-detection-up-revalidation
verified: 2026-04-06T16:00:00Z
status: passed
score: 7/7 must-haves verified
re_verification: false
---

# Phase 43: init-detection-up-revalidation Verification Report

**Phase Goal:** Chrome detection at init time + per-run path revalidation in cmd_up
**Verified:** 2026-04-06
**Status:** PASSED
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | ChromeConfig struct exists in config.rs with chrome_path + mcp_binary_path | VERIFIED | config.rs lines 19-25: `pub struct ChromeConfig { pub chrome_path: PathBuf, pub mcp_binary_path: PathBuf }` |
| 2 | GlobalConfig.chrome: Option<ChromeConfig> field exists | VERIFIED | config.rs line 31: `pub chrome: Option<ChromeConfig>` inside `GlobalConfig` |
| 3 | detect_chrome_binary(), detect_mcp_binary(), detect_chrome_with_home(), detect_chrome() helpers exist in main.rs | VERIFIED | All 5 functions found in main.rs (lines 402-527); brew_prefix() also present, cfg-gated to macOS |
| 4 | --chrome-path CLI arg exists on Init struct | VERIFIED | main.rs lines 110-112: `#[arg(long)] chrome_path: Option<std::path::PathBuf>` on Init variant |
| 5 | cmd_init() has single write_global_config call at end | VERIFIED | main.rs lines 366-370: single `write_global_config(home, &config)?` at end of cmd_init(), after accumulating `tunnel_cfg` and `chrome_cfg` |
| 6 | cmd_up() has per-run path revalidation checking .exists() on both paths, warns + sets chrome_cfg=None if missing | VERIFIED | main.rs lines 693-707: match block with `cfg.chrome_path.exists()` and `cfg.mcp_binary_path.exists()` guards; warn message contains "no longer exists — skipping injection for this run" |
| 7 | cargo build --workspace succeeds | VERIFIED | Build exits 0 in 10.26s; single pre-existing dead_code warning for brew_prefix on non-macOS (cfg-gated, documented in 43-01 SUMMARY) |

**Score:** 7/7 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/rightclaw/src/config.rs` | ChromeConfig struct + GlobalConfig.chrome field + read/write support | VERIFIED | ChromeConfig lines 19-25; GlobalConfig line 31; RawChromeConfig deserializer lines 53-58; write_global_config chrome block lines 131-136 |
| `crates/rightclaw-cli/src/main.rs` | 5 detection helpers + --chrome-path arg + cmd_init single write + cmd_up revalidation | VERIFIED | All elements present and substantive (not stubs) |
| `crates/rightclaw-cli/tests/cli_integration.rs` | test_init_always_writes_config + test_init_chrome_path_arg_warns_when_mcp_missing + test_up_warns_when_chrome_path_missing | VERIFIED (by SUMMARY) | 43-02 SUMMARY reports 23 CLI integration tests pass, 1 pre-existing failure (test_status_no_running_instance) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `global_cfg.chrome.as_ref()` | revalidated effective chrome_cfg | match block checking .exists() on both paths | VERIFIED | main.rs lines 693-707: pattern `chrome_path.exists()` and `mcp_binary_path.exists()` both present |
| effective chrome_cfg | generate_settings() and generate_mcp_config() | per-agent loop | VERIFIED | 43-02 SUMMARY confirms chrome_config param restored to both generator functions; call sites updated |
| detect_chrome() result | GlobalConfig.chrome in written config.yaml | cmd_init accumulation + write_global_config | VERIFIED | main.rs lines 366-370: single write path assembles `GlobalConfig { tunnel: tunnel_cfg, chrome: chrome_cfg }` and writes once |

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|---------|
| CHROME-01 | 43-01 | SATISFIED | detect_chrome_binary() with platform-specific candidates (Linux/macOS cfg-gated) |
| CHROME-02 | 43-01 | SATISFIED | --chrome-path override on Init struct; detect_chrome() accepts override_path param |
| CHROME-03 | 43-01 | SATISFIED | detect_chrome_with_home() warns and returns None when MCP binary missing; non-fatal |
| INJECT-03 | 43-02 | SATISFIED | Per-run revalidation in cmd_up(): both paths checked with .exists(), warn + None on miss |

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| main.rs (brew_prefix) | Dead code warning on non-macOS (cfg-gated, compiler-silent due to `#[cfg(target_os = "macos")]`) | Info | No impact; documented decision: compile on all platforms but only called from macOS cfg branch |

No blockers or warnings found. No TODOs, FIXMEs, placeholder returns, or stub handlers in the phase-modified files.

### Behavioral Spot-Checks

| Behavior | Evidence | Status |
|----------|----------|--------|
| cargo build --workspace compiles clean | Ran build; Finished in 10.26s, exit 0 | PASS |
| Revalidation match has two .exists() checks | grep confirmed chrome_path.exists() line 694 and mcp_binary_path.exists() line 701 | PASS |
| "no longer exists" warn string in both match arms | grep confirmed both arms contain the phrase | PASS |
| "skipping injection for this run" in both arms | grep confirmed 2 occurrences | PASS |
| Single write_global_config call in cmd_init | grep found exactly 1 occurrence in cmd_init body at line 370 | PASS |

### Human Verification Required

None. All behavioral requirements were verifiable via static code analysis and build.

### Gaps Summary

No gaps. All 7 must-haves are verified, the workspace builds clean, and all four requirements (CHROME-01, CHROME-02, CHROME-03, INJECT-03) have supporting implementation evidence.

Notable: The 43-02 plan documented a regression (wave 1 agent accidentally reverted Phase 42 chrome injection code) that was fixed in commit `5f577c3`. The fix restored `generate_settings()` and `generate_mcp_config()` chrome params, 12 chrome-specific tests, and `RC_RIGHTCLAW_HOME` env var. Post-fix test results: 343 lib tests pass, 23 CLI integration tests pass, 1 pre-existing failure (test_status_no_running_instance, documented in STATE.md).

---
_Verified: 2026-04-06T16:00:00Z_
_Verifier: Claude (gsd-verifier)_
