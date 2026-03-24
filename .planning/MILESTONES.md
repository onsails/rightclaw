# Milestones

## v2.0 Native Sandbox & Agent Isolation (Shipped: 2026-03-24)

**Phases completed:** 3 phases, 6 plans, 10 tasks

**Key accomplishments:**

- Stripped all OpenShell code paths -- sandbox.rs replaced by state.rs, policy.yaml removed from init/discovery/doctor, shell wrapper uses single direct-claude path
- v1 backward compatibility test added, all 48 relevant tests pass with zero openshell/sandbox references in codebase
- generate_settings() producing per-agent sandbox JSON with filesystem/network restrictions, security denyRead defaults, and user override merging via SandboxOverrides
- Wired generate_settings() into cmd_up() per-agent loop and refactored init.rs to delegate to shared codegen -- single source of truth for .claude/settings.json
- Linux-specific bwrap/socat binary detection and bwrap smoke test with AppArmor diagnostics in rightclaw doctor
- Replace OpenShell installation with bubblewrap + socat Linux deps and macOS Seatbelt early-return in install.sh

---
