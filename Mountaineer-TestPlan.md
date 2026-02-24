# Mountaineer Live Test Plan (LLM-Friendly)

This plan is designed so another LLM/agent can run and guide testing from a safe working directory (not the project repo path on the external drive).

## Goal
Validate Mountaineer v2 CLI behavior for:
- Stable share paths (`~/Shares/<SHARE>`)
- Failover from Thunderbolt (`tb`) to fallback host
- Auto-failback from fallback to Thunderbolt after stability window
- Alias/favorites/mount/unmount/status flows

## Important Context
- Use the globally available command: `mountaineer`
- Installed binary target: `/Users/I852000/Applications/Mountaineer.app/Contents/MacOS/Mountaineer`
- Do **not** run from `/Volumes/CORE/dev/projects/mac-util-mountaineer`
- Recommended working directory for tests: `/Users/I852000`

## Quick CLI Reference

### Core Commands
- `mountaineer status --all [--json]`
- `mountaineer verify --all [--json]`
- `mountaineer verify --share <SHARE> [--json]`
- `mountaineer reconcile --all`
- `mountaineer monitor --interval <secs>`
- `mountaineer switch --share <SHARE> --to tb|fallback`
- `mountaineer mount-backends --all | --share <SHARE>`
- `mountaineer mount --all`
- `mountaineer unmount --all`

### Favorites
- `mountaineer favorites list [--json]`
- `mountaineer favorites add --share <NAME> --tb-host <IP> --fallback-host <HOST> --username <USER> [--remote-share <REMOTE>]`
- `mountaineer favorites remove --share <NAME> [--cleanup]`

### Aliases
- `mountaineer alias add --name <ALIAS> --share <SHARE> --target-subpath <SUBPATH> [--alias-path <PATH>]`
- `mountaineer alias list [--json]`
- `mountaineer alias reconcile [--all]`
- `mountaineer alias remove --name <ALIAS>`

### Utility
- `mountaineer folders --share <SHARE> [--subpath <DIR>] [--json]`
- `mountaineer install`
- `mountaineer uninstall`

## Known Current Constraint
If a share is already mounted by Finder at `/Volumes/<SHARE>`, mounting that same share to Mountaineer’s managed TB path can return `File exists` and keep `tb.ready=false`.

Practically:
- Keep `/Volumes/CORE` mounted if you need it (safe for active work)
- Validate full failback on a less-critical share (for example `VAULT-R1`) by ensuring `/Volumes/VAULT-R1` is not mounted during TB failback checks

## Preflight Steps
Run in `/Users/I852000`:

```bash
cd /Users/I852000

# Confirm command
command -v mountaineer
mountaineer --version

# Backup runtime config/state
CFG_DIR="/Users/I852000/Library/Application Support/mountaineer"
TS="$(date +%Y%m%d-%H%M%S)"
cp "$CFG_DIR/config.toml" "$CFG_DIR/config.toml.bak.$TS" 2>/dev/null || true
cp "$CFG_DIR/state.json" "$CFG_DIR/state.json.bak.$TS" 2>/dev/null || true
```

## Baseline Setup
Ensure favorites exist:

```bash
mountaineer favorites list

# Example adds/updates
mountaineer favorites add --share CORE --tb-host 10.10.10.1 --fallback-host macmini.local --username "<SMB_USER>" --remote-share CORE
mountaineer favorites add --share VAULT-R1 --tb-host 10.10.10.1 --fallback-host macmini.local --username "<SMB_USER>" --remote-share VAULT-R1
```

Capture baseline:

```bash
mountaineer verify --all --json
mountaineer reconcile --all
mountaineer status --all --json
mount -t smbfs | rg 'CORE|VAULT|10.10.10.1|macmini' || true
ls -la /Users/I852000/Shares
```

## Live Failover Test (User Action Required)
1. Start monitor in one terminal:

```bash
cd /Users/I852000
mountaineer monitor --interval 2
```

2. In another terminal, watch status snapshots:

```bash
cd /Users/I852000
while true; do
  date
  mountaineer status --all --json
  sleep 2
done
```

3. User physically unplugs Thunderbolt.
4. Agent captures:

```bash
mountaineer reconcile --all
mountaineer status --all --json
mount -t smbfs | rg 'CORE|VAULT|10.10.10.1|macmini' || true
ls -la /Users/I852000/Shares
```

Expected:
- Active backend for tested shares switches to `fallback`
- Stable paths remain `~/Shares/<SHARE>` (symlink target changes only)

## Live Failback Test (User Action Required)
1. User reconnects Thunderbolt.
2. Ensure TB auth context is available (if prompted, connect once via Finder to `smb://10.10.10.1/<SHARE>` and save keychain).
3. Wait at least `auto_failback_stable_secs` (default 30s).
4. Run timed checks:

```bash
cd /Users/I852000
for i in 0 10 20 30 40; do
  [ "$i" = "0" ] || sleep 10
  echo "t+$i"
  mountaineer status --all --json
done

# Apply reconcile pass to switch desired->active if needed
mountaineer reconcile --all
mountaineer status --all --json
ls -la /Users/I852000/Shares
mount | rg 'core_tb|core_fb|vault_r1_tb|vault_r1_fb|/Volumes/CORE|/Volumes/VAULT-R1' || true
```

Expected:
- For share(s) without `/Volumes/<SHARE>` conflict, active backend returns to `tb`
- `~/Shares/<SHARE>` points to managed `_tb` mount path

## Conflict-Handling Guidance
When TB is reachable but `tb.ready=false` due `File exists`:
- Check if `/Volumes/<SHARE>` is mounted
- If safe for that share only, unmount it:

```bash
diskutil unmount /Volumes/<SHARE>
```

- Re-run:

```bash
mountaineer reconcile --all
mountaineer status --all --json
```

## Alias Test Flow (Optional)
```bash
mountaineer alias add --name projects-live --share CORE --target-subpath dev/projects --alias-path ~/Shares/Links/projects-live
mountaineer alias list --json
mountaineer alias reconcile --all
mountaineer alias remove --name projects-live
```

Expected:
- Alias remains valid through backend switches because it targets `~/Shares/<SHARE>/...`

## Pass/Fail Checklist
- [x] `mountaineer` command runs outside repo directory
- [x] Favorites configured for target shares
- [x] Failover to fallback confirmed when TB unplugged
- [x] Stable `~/Shares/<SHARE>` paths preserved during failover
- [x] TB reconnect seen as reachable
- [ ] Auto-failback to TB confirmed for at least one non-conflicted share -- **FAIL**
- [~] `reconcile --all` reflects expected active backend transitions -- **PARTIAL** (failover works; failback blocked by auth)
- [x] Logs show transitions/errors clearly

## Log Commands
```bash
tail -n 200 /Users/I852000/Library/Logs/mountaineer.log
```

---

## Test Results (2026-02-19)

**Mountaineer v0.2.0** | Binary: `/usr/local/bin/mountaineer` | Working dir: `/Users/I852000`

### Environment
- Shares tested: CORE, VAULT-R1
- TB host: `10.10.10.1` (Mac Mini over Thunderbolt bridge, Apple Account auth)
- Fallback host: `macmini.local`
- Config/state backed up at `20260219-145803`

### Baseline State (pre-test)

| Share | Active | Desired | TB Reachable | TB Ready | FB Ready | Symlink Target |
|-------|--------|---------|:---:|:---:|:---:|----------------|
| CORE | fallback | fallback | yes | **no** (File exists) | yes | `core_fb` |
| VAULT-R1 | tb | tb | yes | yes | yes | `vault_r1_tb` |

CORE TB mount blocked by Finder-managed `/Volumes/CORE` (known constraint). VAULT-R1 clean on TB -- primary failover/failback candidate.

### Failover Test -- PASS

TB cable physically unplugged. After `reconcile --all`:

| Share | Active | TB Reachable | TB Ready | FB Ready | Symlink Target |
|-------|--------|:---:|:---:|:---:|----------------|
| CORE | fallback | no | no | yes | `core_fb` |
| VAULT-R1 | **fallback** | no | no | yes | **`vault_r1_fb`** |

- VAULT-R1 correctly switched from `tb` to `fallback`
- CORE remained on fallback (was already there)
- Stable paths `~/Shares/CORE` and `~/Shares/VAULT-R1` preserved -- symlink targets updated to `_fb` variants
- SMB mounts confirmed via `mount -t smbfs`: only `macmini.local` mounts active

### Failback Test -- FAIL (auth blocked)

TB cable reconnected. Timed status checks at t+0, t+10, t+20, t+30, t+40:

- TB immediately seen as `reachable: true`
- TB mounts went from `mounted: true` / `alive: true` (at t+0) to `mounted: false` / `alive: false` (by t+20) -- stale mount handles expired
- `desired_backend` remained `fallback` throughout -- auto-failback never triggered

`mount-backends --all` and `reconcile --all` both failed to re-establish TB mounts:

```
VAULT-R1 tb mount failed: mount_smbfs failed (exit 77):
  mount_smbfs: server rejected the connection: Authentication error
```

`mountaineer switch --share VAULT-R1 --to tb` also failed:
```
Error: cannot switch 'VAULT-R1' to tb: backend is not ready
```

**Root cause investigation:**
1. `mount_smbfs` (exit 77) -- Apple Account (SPNEGO) auth tokens invalidated by TB disconnect
2. Keychain entry for `10.10.10.1` has `acct="No user account"` (Finder/SPNEGO path), no entry matching Mountaineer's configured Apple ID username
3. `osascript` mount (`tell application "Finder" to mount volume "smb://10.10.10.1/VAULT-R1"`) succeeds but always mounts to `/Volumes/VAULT-R1`, not to Mountaineer's managed path `~/.mountaineer/mnts/vault_r1_tb`
4. Tray app logs confirm it uses `osascript` fallback (`"Mounted //macmini.local/VAULT-R1 via osascript"`), but CLI commands do not

### Alias Test -- PASS

```
mountaineer alias add --name projects-live --share CORE --target-subpath dev/projects
  -> healthy: true, target_exists: true
  -> symlink: ~/Shares/Links/projects-live -> ~/Shares/CORE/dev/projects

mountaineer alias list --json         -> 1 alias, healthy
mountaineer alias reconcile --all     -> 1 alias, healthy
mountaineer alias remove --name projects-live -> removed, symlink cleaned up
```

### Critical Finding: CLI TB re-mount auth failure after reconnect

**Severity:** High -- blocks auto-failback entirely in Apple Account auth environments

**Problem:** After TB cable disconnect/reconnect, the CLI (`mount-backends`, `reconcile`, `switch`) cannot re-establish TB mounts. `mount_smbfs` returns exit 77 (Authentication error) because Apple Account SPNEGO tokens are invalidated when the physical connection drops.

**Scope:** Affects all CLI mount paths. The tray app has an `osascript` fallback but it mounts to `/Volumes/<SHARE>` (Finder-managed), not to `~/.mountaineer/mnts/<share>_tb` (Mountaineer-managed). This means even the tray app fallback doesn't solve the managed-path requirement.

**Impact:** After any TB cable reconnect cycle, all shares remain on fallback permanently. Manual intervention cannot restore TB via CLI. The only workaround observed is restarting the tray app (which re-mounts via osascript at `/Volumes/` paths, separate from managed mounts).

**Suggested fixes:**
1. Add `osascript` mount fallback to CLI mount path (matching tray app behavior)
2. After osascript mounts at `/Volumes/<SHARE>`, use `mount_smbfs` with `--move` or bind-mount to relocate to the managed path
3. Alternatively, detect existing `/Volumes/<SHARE>` mounts and adopt them as the TB backend mount (update state to point there instead of requiring `~/.mountaineer/mnts/<share>_tb`)
4. Consider using `smbutil` or Kerberos ticket refresh to re-establish SPNEGO auth before attempting `mount_smbfs`

## Agent Prompt Template
Use this in a new chat with another LLM:

"You are helping run `~/Desktop/Mountaineer-TestPlan.md` step-by-step. Run commands yourself, pause only when I need to unplug/reconnect cable or approve a risky unmount, and summarize observed status transitions with exact command outputs. Keep CORE conflict-safe unless I explicitly allow unmounting `/Volumes/CORE`."

---

## Retest Results (2026-02-19 15:26 -- post-update)

**Mountaineer v0.2.0** (binary updated 2026-02-19 15:22:45, sha1 prefix `3549bd8e7e95`) | Binary: `/usr/local/bin/mountaineer` | Working dir: `/Users/I852000`

### Environment
- Shares tested: CORE, VAULT-R1
- TB host: `10.10.10.1` (Mac Mini over Thunderbolt bridge, Apple Account auth)
- Fallback host: `macmini.local`
- Config/state backed up at `20260219-152635`
- Previous test run: 2026-02-19 14:58 (v0.2.0 pre-update) -- see results above

### What Changed in the Update

The update added **osascript mount fallback with symlink adoption** to the CLI mount path. When `mount_smbfs` fails (e.g., due to `File exists` or auth error), the CLI now:
1. Attempts `osascript` mount via Finder (mounts at `/Volumes/<SHARE>`)
2. Creates a symlink from the managed path (`~/.mountaineer/mnts/<share>_<backend>`) to the `/Volumes/<SHARE>` mount

This directly addresses the critical finding from the previous test (CLI TB re-mount auth failure after reconnect).

### Baseline State (pre-test)

| Share | Active | Desired | TB Reachable | TB Mounted | TB Ready | FB Ready | Symlink Target |
|-------|--------|---------|:---:|:---:|:---:|:---:|----------------|
| CORE | fallback | **tb** | yes | yes | yes | no | `core_tb` -> `/Volumes/CORE` |
| VAULT-R1 | fallback | **tb** | yes | yes | yes | yes | `vault_r1_tb` (real mount) |

Key differences from previous baseline:
- `desired_backend` is now `tb` for both shares (was `fallback` previously) -- the update detected TB readiness and set desire to failback
- CORE TB: `mounted=true, ready=true` via symlink adoption (`core_tb -> /Volumes/CORE`) -- previously was `ready=false` due to `File exists` conflict
- CORE FB: `mounted=false, ready=false` -- fallback mount failing (see bugs below)

After `reconcile --all`, both shares switched to `active=tb`:

```
CORE             tb          yes      yes      yes      no       ~/Shares/CORE
  ! CORE fallback mount failed: mount_smbfs failed (exit 64): ...File exists;
    osascript fallback mounted no detectable share path
VAULT-R1         tb          yes      yes      yes      yes      ~/Shares/VAULT-R1
```

Managed mount directory state post-reconcile:
```
core_fb      -> empty dir (mount failed)
core_tb      -> symlink to /Volumes/CORE
vault_r1_fb  -> real SMB mount (macmini.local)
vault_r1_tb  -> real SMB mount (10.10.10.1)
```

### Failover Test -- PASS

TB cable physically unplugged.

**Pre-reconcile status** (captured immediately after unplug):

| Share | Active | Desired | TB Reachable | TB Ready | FB Ready |
|-------|--------|---------|:---:|:---:|:---:|
| CORE | tb | **null** | no | no | no |
| VAULT-R1 | tb | **fallback** | no | no | yes |

New behavior: CORE `desired_backend=null` (was always set previously). VAULT-R1 already detected TB down and set desired to fallback proactively.

**Post-reconcile:**

| Share | Active | TB Net | TB Mnt | TB Ready | FB Net | FB Mnt | FB Ready |
|-------|--------|:---:|:---:|:---:|:---:|:---:|:---:|
| CORE | **fallback** | no | no | no | yes | yes | yes |
| VAULT-R1 | **fallback** | no | no | no | yes | yes | yes |

- Both shares correctly failed over to fallback
- Stable paths preserved: `~/Shares/CORE -> core_fb`, `~/Shares/VAULT-R1 -> vault_r1_fb`
- CORE fallback mount succeeded during failover (was failing earlier with `File exists`)
- Note: TB status shows `mounted=true, alive=true` but `ready=false` -- stale mount handles not yet cleaned up

**SMB mounts during failover:**
```
macmini.local/VAULT-R1 on ~/.mountaineer/mnts/vault_r1_fb  (active)
macmini.local/CORE     on ~/.mountaineer/mnts/core_fb       (active)
10.10.10.1/CORE        on /Volumes/CORE                     (stale)
10.10.10.1/VAULT-R1    on ~/.mountaineer/mnts/vault_r1_tb   (stale)
```

### Failback Test -- PASS (previously FAIL)

TB cable reconnected. Timed compact status checks:

```
t+0:   CORE: active=fallback desired=fallback tb.reachable=True  tb.ready=False fb.ready=True
       VAULT: active=fallback desired=fallback tb.reachable=True  tb.ready=False fb.ready=True

t+10:  CORE: active=tb       desired=fallback tb.reachable=True  tb.ready=False fb.ready=True
       VAULT: active=fallback desired=fallback tb.reachable=True  tb.ready=False fb.ready=True

t+20:  CORE: active=tb       desired=fallback tb.reachable=True  tb.ready=False fb.ready=True
       VAULT: active=tb       desired=tb       tb.reachable=True  tb.ready=True  fb.ready=True

t+30:  CORE: active=tb       desired=tb       tb.reachable=True  tb.ready=True  fb.ready=True
       VAULT: active=tb       desired=tb       tb.reachable=True  tb.ready=True  fb.ready=True

t+40:  (stable -- same as t+30)
```

Post-reconcile final state:
```
CORE             tb     yes  yes  yes  yes  ~/Shares/CORE
VAULT-R1         tb     yes  yes  yes  yes  ~/Shares/VAULT-R1
```

**Stable paths after failback:**
```
~/Shares/CORE     -> ~/.mountaineer/mnts/core_tb     (symlink -> /Volumes/CORE)
~/Shares/VAULT-R1 -> ~/.mountaineer/mnts/vault_r1_tb (symlink -> /Volumes/VAULT-R1)
```

**SMB mounts after failback:**
```
macmini.local/VAULT-R1 on ~/.mountaineer/mnts/vault_r1_fb   (standby)
macmini.local/CORE     on ~/.mountaineer/mnts/core_fb       (standby)
10.10.10.1/VAULT-R1    on /Volumes/VAULT-R1                 (active via symlink)
10.10.10.1/CORE        on /Volumes/CORE                     (active via symlink)
```

Both `_tb` managed paths are now symlinks to `/Volumes/` Finder mounts (osascript adoption). Both fallback mounts remain as real mounts at managed paths.

### Alias Test -- PASS

All operations identical to previous test. Alias correctly targets through stable path chain: `~/Shares/Links/projects-live -> ~/Shares/CORE/dev/projects -> core_tb -> /Volumes/CORE/dev/projects`.

### Monitor Output Observations

The background `mountaineer monitor --interval 2` captured:
- CORE FB mount repeatedly failing with `File exists` + `osascript fallback mounted no detectable share path` (5 cycles)
- One cycle showed osascript error: `execution error: An error of type -5014 has occurred. (-5014)`
- Then FB mount succeeded and stabilized
- Monitor did not capture failover/failback transitions in its output (the manual `reconcile` and `status` commands handled those)

### Retest Pass/Fail Checklist

- [x] `mountaineer` command runs outside repo directory
- [x] Favorites configured for target shares
- [x] Failover to fallback confirmed when TB unplugged
- [x] Stable `~/Shares/<SHARE>` paths preserved during failover
- [x] TB reconnect seen as reachable
- [x] **Auto-failback to TB confirmed** (previously FAIL -- now PASS via osascript + symlink adoption)
- [x] `reconcile --all` reflects expected active backend transitions
- [~] Logs show transitions/errors clearly -- **PARTIAL** (see bug #3 below)

### Bugs Found

**Bug #1 (Medium): Premature active backend switch before TB ready**

During failback at t+10, CORE showed:
```
active=tb  desired=fallback  tb.mounted=False  tb.alive=False  tb.ready=False
```
The active backend was switched to `tb` before TB was actually ready (mounted/alive/ready all false), and while desired was still `fallback`. This transient state could cause brief I/O failures for applications reading through the stable path.

**Reproduction:** Reconnect TB cable and poll `status --all --json` every 10 seconds. Observe that `active_backend` may flip to `tb` before `tb.ready=true`.

**Bug #2 (Medium): CORE fallback mount fails intermittently with `File exists` + osascript errors**

During baseline and monitor cycles, CORE fallback mount repeatedly failed:
```
mount_smbfs failed (exit 64): ...@macmini.local/CORE: File exists;
osascript fallback mounted no detectable share path
```
And once with:
```
osascript fallback failed: execution error: An error of type -5014 has occurred. (-5014)
```

This occurs when TB is the active backend for CORE and the fallback mount to `macmini.local/CORE` is attempted. The `mount_smbfs` `File exists` error suggests macOS SMB layer is refusing a duplicate share mount (same share name from a different host while `/Volumes/CORE` exists). The osascript fallback also fails because it can't create a distinguishable mount point.

The mount eventually succeeds (observed in monitor output and during failover), but the repeated errors are noisy and could delay failover response time.

**Bug #3 (Low): CLI operations not logged to mountaineer.log**

`mountaineer.log` at `~/Library/Logs/mountaineer.log` only contains tray app events. CLI commands (`reconcile`, `mount-backends`, `status`, `verify`, `switch`, `monitor`) do not write to this log. This makes post-mortem analysis of CLI-driven failover/failback events impossible from logs alone.

**Bug #4 (Low): `desired_backend=null` for CORE during failover**

Immediately after TB disconnect, CORE showed `desired_backend: null` while VAULT-R1 showed `desired_backend: "fallback"`. The null value is unexpected and inconsistent. CORE eventually resolved to `desired_backend: "fallback"` after reconcile, but the transient null state could confuse monitoring/alerting systems.

**Bug #5 (Informational): Stale TB mount handles persist after disconnect**

After TB cable disconnect, both shares show `tb.mounted=true, tb.alive=true` despite `tb.reachable=false`. These are stale kernel mount handles. They eventually become `mounted=false` but the delay (observed ~20 seconds in previous test) means status output is temporarily misleading. The `ready=false` field correctly reflects the true state, but `mounted` and `alive` do not.

**Bug #6 (Informational): `vault_r1_tb` changed from real mount to symlink after failback**

Before the test, `vault_r1_tb` was a real SMB mount directory. After the failover/failback cycle, it became a symlink to `/Volumes/VAULT-R1`. This is functionally correct but means the mount topology changes after each failover/failback cycle (real mount -> symlink). The original real mount was more robust since it didn't depend on the `/Volumes/` Finder path remaining stable.

### Summary: Previous vs Current

| Check | v0.2.0 (pre-update) | v0.2.0 (post-update) |
|-------|:---:|:---:|
| Failover | PASS | PASS |
| Stable paths preserved | PASS | PASS |
| TB reconnect detected | PASS | PASS |
| Auto-failback to TB | **FAIL** | **PASS** |
| Reconcile transitions | PARTIAL | PASS |
| Alias flow | PASS | PASS |
| CLI logging | N/A | PARTIAL (not logged) |

The critical auth failure blocking failback is resolved. The osascript + symlink adoption approach successfully works around macOS `mount_smbfs` SPNEGO limitations. Remaining bugs are lower severity (transient state inconsistencies, noisy fallback mount errors, missing CLI logging).

---

## Test Run #3 (2026-02-19 16:27 — regression check)

**Mountaineer v0.2.0** | Binary: `/usr/local/bin/mountaineer` | Working dir: `/Users/I852000`

### Environment
- Shares tested: CORE, VAULT-R1
- TB host: `10.10.10.1` (Mac Mini over Thunderbolt bridge, Apple Account auth)
- Fallback host: `macmini.local`
- Config/state backed up at `20260219-162702`
- Previous test run: Retest (2026-02-19 15:26, post-update) — see results above

### Baseline State (pre-test)

| Share | Active | Desired | TB Reachable | TB Mounted | TB Ready | FB Mounted | FB Ready |
|-------|--------|---------|:---:|:---:|:---:|:---:|:---:|
| CORE | tb | tb | yes | yes | yes | yes | yes |
| VAULT-R1 | tb | tb | yes | yes | yes | yes | yes |

All backends healthy. Both TB mounts via symlink adoption (osascript path):
```
core_tb      -> symlink to /Volumes/CORE
core_fb      -> real SMB mount (macmini.local)
vault_r1_tb  -> symlink to /Volumes/VAULT-R1
vault_r1_fb  -> real SMB mount (macmini.local)
```

Stable paths:
```
~/Shares/CORE     -> ~/.mountaineer/mnts/core_tb -> /Volumes/CORE
~/Shares/VAULT-R1 -> ~/.mountaineer/mnts/vault_r1_tb -> /Volumes/VAULT-R1
```

### Failover Test — PASS

TB cable physically unplugged.

**Pre-reconcile status** (captured immediately after unplug):

| Share | Active | Desired | TB Reachable | TB Mounted | TB Alive | TB Ready | FB Reachable | FB Mounted | FB Ready |
|-------|--------|---------|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| CORE | tb | **fallback** | no | yes* | yes* | no | yes | yes | yes |
| VAULT-R1 | tb | **fallback** | no | yes* | yes* | no | **no** | yes | **no** |

*stale kernel handles

Notable: VAULT-R1 fallback transiently showed `reachable=false, ready=false` — possible `macmini.local` DNS resolution stall. Recovered by reconcile time.

Bug #4 (`desired_backend=null`) NOT reproduced — CORE went straight to `desired=fallback`.

**Post-reconcile:**

| Share | Active | TB Net | TB Mnt | FB Net | FB Mnt | Stable Path |
|-------|--------|:---:|:---:|:---:|:---:|-------------|
| CORE | **fallback** | no | no | yes | yes | ~/Shares/CORE |
| VAULT-R1 | **fallback** | no | no | yes | yes | ~/Shares/VAULT-R1 |

- Both shares correctly failed over to fallback
- Stable paths preserved: `~/Shares/CORE -> core_fb`, `~/Shares/VAULT-R1 -> vault_r1_fb`
- SMB mounts confirmed: only `macmini.local` mounts active; stale `/Volumes/CORE` and `/Volumes/VAULT-R1` TB mounts still in kernel mount table

### Failback Test — FAIL (auto-failback regression)

TB cable reconnected. Timed compact status checks:

```
t+0  (16:41:46):  CORE:     active=fallback desired=fallback tb.reachable=true  tb.mounted=false tb.ready=false fb.ready=true
                  VAULT-R1: active=fallback desired=fallback tb.reachable=true  tb.mounted=false tb.ready=false fb.ready=true

t+10 (16:41:59):  (same as t+0 — no change)

t+20 (16:42:12):  (same as t+0 — no change)

t+30 (16:42:25):  (same as t+0 — no change)

t+40 (16:42:38):  (same as t+0 — no change)
```

TB was `reachable=true` immediately but `mounted=false, ready=false` throughout the entire 40-second window. `desired_backend` never flipped to `tb`. Auto-failback did not trigger.

**Manual intervention required:**

1. `mount-backends --all` — successfully re-established TB mounts (both went to `TB MNT=yes`)
2. `reconcile --all` — did **NOT** switch active to `tb`; both shares remained `active=fallback, desired=fallback` despite TB being `reachable=true, mounted=true, alive=true, ready=true`
3. `switch --share VAULT-R1 --to tb` — **succeeded** (manual force)
4. `switch --share CORE --to tb` — **succeeded** (manual force)

**Final state after manual switch:**

| Share | Active | Desired | TB Ready | FB Ready | Symlink Target |
|-------|--------|---------|:---:|:---:|----------------|
| CORE | tb | tb | yes | yes | `core_tb` -> `/Volumes/CORE` |
| VAULT-R1 | tb | tb | yes | yes | `vault_r1_tb` -> `/Volumes/VAULT-R1` |

```
~/Shares/CORE     -> ~/.mountaineer/mnts/core_tb (symlink -> /Volumes/CORE)
~/Shares/VAULT-R1 -> ~/.mountaineer/mnts/vault_r1_tb (symlink -> /Volumes/VAULT-R1)
```

SMB mounts after failback:
```
macmini.local/VAULT-R1 on ~/.mountaineer/mnts/vault_r1_fb   (standby)
macmini.local/CORE     on ~/.mountaineer/mnts/core_fb       (standby)
10.10.10.1/CORE        on /Volumes/CORE                     (active via symlink)
10.10.10.1/VAULT-R1    on /Volumes/VAULT-R1                 (active via symlink)
```

### Alias Test — PASS

```
mountaineer alias add --name projects-live --share CORE --target-subpath dev/projects
  -> healthy: true, target_exists: true
  -> symlink: ~/Shares/Links/projects-live -> ~/Shares/CORE/dev/projects

mountaineer alias list --json         -> 1 alias, healthy
mountaineer alias reconcile --all     -> 1 alias, healthy, no repairs
mountaineer alias remove --name projects-live -> removed, symlink cleaned up
```

### Pass/Fail Checklist

- [x] `mountaineer` command runs outside repo directory
- [x] Favorites configured for target shares
- [x] Failover to fallback confirmed when TB unplugged
- [x] Stable `~/Shares/<SHARE>` paths preserved during failover
- [x] TB reconnect seen as reachable
- [ ] Auto-failback to TB confirmed — **FAIL** (regression from previous test)
- [~] `reconcile --all` reflects expected active backend transitions — **PARTIAL** (failover yes; failback requires manual `switch`)
- [x] Alias flow (add/list/reconcile/remove)
- [~] Logs show transitions/errors clearly — **PARTIAL** (CLI invocations logged, no detail)

### Bugs Found / Updated

**Bug #7 (High — NEW): Auto-failback regression — `desired_backend` stuck on `fallback` after TB reconnect**

After TB cable reconnect, the CLI auto-failback mechanism does not trigger:

1. `tb.reachable=true` immediately, but `tb.mounted=false, tb.ready=false` persists indefinitely — the `status` command does not attempt mounts
2. `mount-backends --all` is needed to re-establish TB mounts
3. Even after TB is fully ready (`reachable/mounted/alive/ready` all true), `reconcile --all` does NOT update `desired_backend` from `fallback` to `tb`
4. Only `switch --share <X> --to tb` forces the transition

**Regression from previous test run** (Retest 2026-02-19 15:26), where auto-failback worked by t+10–t+30 without manual intervention. Possible causes:
- Auto-failback timer logic may only run inside `monitor` loop, not in `status`/`reconcile` CLI commands
- The `reconcile` command may not re-evaluate `desired_backend` when TB becomes ready after a failover — it only acts on the current desired state
- State file may have a stale `desired_backend=fallback` that reconcile respects without checking whether TB readiness warrants an upgrade

**Suggested fix:** `reconcile --all` should include auto-failback evaluation: if `auto_failback=true`, TB is ready, TB has been stable for `auto_failback_stable_secs`, and `desired_backend=fallback`, then set `desired_backend=tb` and switch.

**Bug #5 (Informational — confirmed):** Stale TB mount handles persist after disconnect. `tb.mounted=true, tb.alive=true` with `tb.reachable=false` seen in pre-reconcile snapshot. `ready=false` is correct.

**Bug #3 (Low — partially improved):** CLI commands now log invocation lines (e.g., `cli: reconcile --all=true`) to `mountaineer.log`. However, no mount attempt details, state transitions, or error messages are logged from CLI path. Tray app events still have full detail. Post-mortem analysis of CLI-driven failover/failback still limited.

**Bug #4 (Low — NOT reproduced):** `desired_backend=null` for CORE during failover was not observed in this run. CORE went directly to `desired_backend=fallback`.

**Bug #8 (Low — NEW): VAULT-R1 fallback transiently unreachable during TB disconnect**

Immediately after TB cable unplug, VAULT-R1 fallback showed `reachable=false, ready=false` in the pre-reconcile snapshot, while CORE fallback was `reachable=true, ready=true`. Both use `macmini.local` as fallback host. The transient unreachability resolved by the time `reconcile --all` ran (~15s later). Likely caused by `macmini.local` mDNS resolution stalling during the network topology change from TB disconnect.

### Summary: Test Run Comparison

| Check | Run #1 (pre-update) | Run #2 (post-update) | Run #3 (this run) |
|-------|:---:|:---:|:---:|
| Failover | PASS | PASS | PASS |
| Stable paths preserved | PASS | PASS | PASS |
| TB reconnect detected | PASS | PASS | PASS |
| Auto-failback to TB | **FAIL** (auth) | **PASS** | **FAIL** (regression) |
| Reconcile transitions | PARTIAL | PASS | PARTIAL |
| Alias flow | PASS | PASS | PASS |
| CLI logging | N/A | PARTIAL | PARTIAL |

The auto-failback that was working in Run #2 has regressed. The key difference: in Run #2, `desired_backend` and `active_backend` both flipped to `tb` progressively during the timed status checks (by t+10–t+30). In this run, neither flipped at all over 40+ seconds, and even `reconcile --all` with TB fully ready did not trigger the switch. This suggests the auto-failback mechanism is unreliable or depends on the `monitor` loop running concurrently.
