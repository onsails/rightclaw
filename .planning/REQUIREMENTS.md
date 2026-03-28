# Requirements: v2.4 Sandbox Telegram Fix

## Milestone Goal

Diagnose and fix why CC sandbox blocks Telegram message processing, so agents respond to Telegram commands whether sandbox is enabled or not.

## Active Requirements

### Diagnosis

- [ ] **DIAG-01**: Developer can identify why CC stops processing Telegram events when sandbox is enabled by analyzing right-debug.log
- [ ] **DIAG-02**: Root cause is confirmed as sandbox-specific (log comparison: sandbox on vs --no-sandbox)
- [ ] **DIAG-03**: Specific config element responsible is identified (bwrap network rules, socat relay, or settings.json network/filesystem section)

### Fix

- [ ] **FIX-01**: Telegram commands receive responses from agent when sandbox is enabled (Linux/bwrap)
- [ ] **FIX-02**: Fix does not regress --no-sandbox behavior or existing test suite

### Verification

- [ ] **VERIFY-01**: Manual end-to-end test: send Telegram message → agent responds with sandbox on

## Future Requirements

- Automated regression test for sandbox + Telegram (deferred — manual verification sufficient for v2.4)

## Out of Scope

- macOS Seatbelt Telegram behavior (not known to be broken)
- Windows support
- Fixing rightcron background agent session hang (separate issue, different root cause)

## Traceability

| Requirement | Phase | Plan |
|-------------|-------|------|
| DIAG-01 | TBD | TBD |
| DIAG-02 | TBD | TBD |
| DIAG-03 | TBD | TBD |
| FIX-01 | TBD | TBD |
| FIX-02 | TBD | TBD |
| VERIFY-01 | TBD | TBD |
