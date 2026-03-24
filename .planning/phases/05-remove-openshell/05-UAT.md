---
status: complete
phase: v2.0 (phases 05-07 combined)
source: [05-01-SUMMARY.md, 05-02-SUMMARY.md, 06-01-SUMMARY.md, 06-02-SUMMARY.md, 07-01-SUMMARY.md, 07-02-SUMMARY.md]
started: 2026-03-24T16:00:00Z
updated: 2026-03-24T16:10:00Z
---

## Current Test

[testing complete]

## Tests

### 1. rightclaw init generates sandbox settings.json
expected: `rightclaw init` creates `.claude/settings.json` with sandbox.enabled, filesystem restrictions, network allowlist, secure defaults
result: pass
notes: Settings.json generated with all expected fields. allowWrite points to absolute agent dir path. 6 default domains present. denyRead for ~/.ssh, ~/.aws, ~/.gnupg.

### 2. rightclaw doctor checks bwrap/socat on Linux
expected: `rightclaw doctor` shows bwrap and socat checks with FAIL when not in PATH
result: pass
notes: Doctor correctly detects missing bwrap/socat with install guidance (apt/dnf/pacman).

### 3. rightclaw doctor does NOT check for openshell
expected: Doctor output has no openshell mention
result: pass
notes: 5 binary checks + agent validation. Zero openshell.

### 4. bwrap smoke test works when available
expected: `bwrap --ro-bind / / --unshare-net --dev /dev true` succeeds on this system
result: pass
notes: Tested via `nix run nixpkgs#bubblewrap`. AppArmor not blocking.

### 5. --no-sandbox generates sandbox.enabled=false
expected: settings.json has sandbox.enabled=false when --no-sandbox flag used
result: pass
notes: Tested via `rightclaw up --no-sandbox` in devenv. settings.json confirmed sandbox.enabled=false.

### 6. Shell wrapper launches claude directly
expected: Generated wrapper has `exec "$CLAUDE_BIN"` with no openshell
result: pass
notes: Wrapper at run/right.sh confirmed — direct exec, --dangerously-skip-permissions, --model, startup prompt.

### 7. rightclaw down does not destroy sandboxes
expected: Just stops process-compose, no sandbox destroy
result: pass
notes: Output: "All agents stopped." No sandbox destroy attempt.

### 8. agent.yaml sandbox overrides parsed correctly
expected: agent.yaml with sandbox: section parses without error
result: pass
notes: Tested with allow_write, allowed_domains fields. `rightclaw list` shows "config: yes", exit 0.

### 9. devenv includes sandbox dependencies
expected: devenv.nix includes bubblewrap and socat for development
result: issue
reported: "bwrap and socat not in devenv.nix PATH — devenv wasn't updated for v2.0 deps"
severity: minor

## Summary

total: 9
passed: 8
issues: 1
pending: 0
skipped: 0
blocked: 0

## Gaps

- truth: "devenv.nix includes bubblewrap and socat for development"
  status: failed
  reason: "User reported: bwrap and socat not in devenv.nix PATH — devenv wasn't updated for v2.0 deps"
  severity: minor
  test: 9
  artifacts: [devenv.nix]
  missing: [bubblewrap, socat packages in devenv inputs]
