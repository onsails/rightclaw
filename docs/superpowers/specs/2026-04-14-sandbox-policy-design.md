# Future-Proof Sandbox Filesystem Policy

## Problem

1. Filesystem policy overlays are baked at sandbox creation — `openshell policy set` does NOT
   hot-reload filesystem changes. Adding a new writable path (e.g. `/platform`) requires sandbox
   recreation, which destroys agent data.
2. Current policy doesn't include `/platform` — new sandboxes get it, old ones don't.
3. Agents can't install CLI tools to standard locations (`/usr/local/bin` is root-owned).

## Solution

Minimal, future-proof policy. Only add `/platform` to read_write. Agent tool installation
uses `$HOME/.local/bin` (`/sandbox/.local/bin`) which is already writable. Ensure it's in PATH.

## Policy

```yaml
filesystem_policy:
  include_workdir: true
  read_only:
    - /usr
    - /lib
    - /lib64
    - /etc
    - /proc
    - /dev/urandom
    - /dev/null
  read_write:
    - /tmp
    - /sandbox
    - /platform
```

## PATH for agent-installed tools

Add `/sandbox/.local/bin` to PATH in the sandbox `.bashrc`. This is where `curl | bash`
installers put binaries when they target `$HOME/.local/bin`.

Current `.bashrc` (in `/sandbox/.bashrc`):
```bash
export PATH="/sandbox/.venv/bin:/usr/local/bin:/usr/bin:/bin"
```

New `.bashrc`:
```bash
export PATH="/sandbox/.local/bin:/sandbox/.venv/bin:/usr/local/bin:/usr/bin:/bin"
```

This is set during staging/initial_sync — write `.bashrc` to sandbox if missing or
if `/sandbox/.local/bin` not in PATH.

## Migration for existing sandboxes

The `/platform` path requires sandbox recreation — no workaround exists in OpenShell.
Bot must detect this on startup:
1. `exec_command(sandbox, &["mkdir", "-p", "/platform"])` 
2. If exit code != 0 → sandbox filesystem is stale
3. Log clear error: "Sandbox filesystem policy outdated. Run `rightclaw agent recreate <name>` to update."
4. Fall back to direct upload (old sync behavior) so bot still works

This preserves agent data while giving the operator a clear signal that recreation is needed.

## Files changed

- `crates/rightclaw/src/codegen/policy.rs` — already has `/platform` (done in earlier task)
- `crates/rightclaw/src/platform_store.rs` — graceful fallback when `/platform` unavailable
- `crates/bot/src/sync.rs` — ensure `.bashrc` has `/sandbox/.local/bin` in PATH
