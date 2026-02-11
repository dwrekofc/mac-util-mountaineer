# Mountaineer — macOS Menu Bar SMB Drive Manager

## Context

When a Mac connects to a network with both Ethernet and Wi-Fi, macOS does not seamlessly keep SMB shares mounted on the fastest interface. Unplugging Ethernet kills the SMB connection; macOS won't remount via Wi-Fi. Plugging back in doesn't migrate the connection to Ethernet. The user wants a menu bar utility that **keeps configured SMB drives always mounted on the best available interface**, automatically handling failover and failback — without manual intervention.

## Stack

| Role | Crate / Tool |
|---|---|
| UI windows/panels | `gpui` |
| Menu bar status item + native menu | `tray-icon` (re-exports `muda`) |
| Network change detection | `system-configuration` (SCDynamicStore) |
| Device discovery | `mdns-sd` (Bonjour/mDNS) |
| Keychain credentials | `security-framework` |
| Config persistence | `serde` + `toml` |
| Async runtime | GPUI's built-in executor (GCD-backed) |
| App bundling | `cargo-bundle` |
| SMB mount/unmount | Shell out to `mount_smbfs` / `diskutil unmount` |
| Share enumeration | Shell out to `smbutil view` (lists shares on a host) |
| CLI argument parsing | `clap` (derive) |
| CLI ↔ GUI IPC | Unix domain socket (`/tmp/mountaineer.sock`) |

## Project Structure

```
network-drive-mngr/
├── Cargo.toml
├── assets/
│   ├── Info.plist.ext          # LSUIElement = true
│   ├── icon-connected.png      # Green status icon
│   ├── icon-degraded.png       # Yellow status icon
│   ├── icon-disconnected.png   # Red/gray status icon
│   └── icon-idle.png           # No drives configured
├── src/
│   ├── main.rs                 # Entry point: CLI dispatch or GUI launch
│   ├── app_state.rs            # Central AppState (Global), shared across all components
│   ├── cli/
│   │   ├── mod.rs              # Clap CLI definition + dispatch
│   │   ├── commands.rs         # CLI command handlers (standalone or IPC-to-GUI)
│   │   └── ipc.rs              # Unix socket client (send commands to running GUI)
│   ├── ipc_server.rs           # Unix socket server (GUI listens for CLI commands)
│   ├── tray.rs                 # Tray icon lifecycle, menu construction, event polling
│   ├── network/
│   │   ├── mod.rs
│   │   ├── monitor.rs          # SCDynamicStore listener → channel → GPUI
│   │   ├── scanner.rs          # mDNS browse for SMB services
│   │   └── interface.rs        # Enumerate interfaces, detect type (Ethernet/WiFi), get IPs
│   ├── mount/
│   │   ├── mod.rs
│   │   ├── manager.rs          # Mount decision engine: evaluate + reconcile
│   │   ├── smb.rs              # Low-level mount_smbfs / diskutil unmount wrappers
│   │   └── adhoc.rs            # Ad-hoc (one-click) mount/unmount for unmanaged shares
│   ├── config/
│   │   ├── mod.rs
│   │   └── store.rs            # Load/save TOML config, drive definitions
│   ├── keychain.rs             # Store/retrieve SMB credentials from macOS Keychain
│   └── ui/
│       ├── mod.rs
│       ├── drive_list.rs       # Main dashboard panel (drive statuses)
│       ├── scanner.rs          # Network scanner results panel
│       └── settings.rs         # Preferences window
└── launchd/
    └── com.mountaineer.agent.plist  # LaunchAgent for login startup
```

## Architecture & Data Flow

```
┌──────────────────────────────────────────────────────────┐
│                    GPUI Application                       │
│                                                          │
│  ┌─────────┐    AppState (Global)    ┌───────────────┐   │
│  │  Tray   │◄──── drives, status ───►│  GPUI Windows │   │
│  │  Icon   │     interfaces          │  (Dashboard,  │   │
│  │  +Menu  │     config              │   Scanner,    │   │
│  └────┬────┘                         │   Settings)   │   │
│       │ MenuEvent                    └───────────────┘   │
│       ▼                                                  │
│  ┌──────────┐  network    ┌───────────┐   mount/unmount  │
│  │ Network  │──change──►  │  Mount    │──────►  shell    │
│  │ Monitor  │  event      │  Manager  │   mount_smbfs    │
│  └──────────┘             └───────────┘   diskutil       │
│                                                          │
│  ┌──────────┐             ┌───────────┐                  │
│  │  mDNS    │             │ Keychain  │                  │
│  │ Scanner  │             │ Manager   │                  │
│  └──────────┘             └───────────┘                  │
└──────────────────────────────────────────────────────────┘
```

### AppState (the central hub)

```rust
struct AppState {
    // Managed drives (auto-connect, failover)
    drives: Vec<DriveConfig>,
    drive_statuses: HashMap<DriveId, DriveStatus>,

    // Ad-hoc mounts (one-click from browser, not auto-managed)
    adhoc_mounts: HashMap<String, AdhocMount>,  // key = mount_point

    // Network state
    interfaces: Vec<NetworkInterface>,

    // Discovery
    discovered_hosts: Vec<DiscoveredHost>,       // mDNS results (computers on network)
    scanner_state: ScannerState,                 // Idle / Scanning / Done(timestamp)

    // Config
    config: Config,
}

struct DiscoveredHost {
    hostname: String,                // e.g. "macmini.local"
    ip_addresses: Vec<IpAddr>,       // All resolved IPs
    shares: Vec<DiscoveredShare>,    // Enumerated via smbutil
    last_seen: Instant,
}

struct DiscoveredShare {
    name: String,                    // "SharedData", "TimeMachine", etc.
    is_managed: bool,                // Already in drives[] config?
    is_mounted: bool,                // Currently mounted (managed or ad-hoc)?
}

struct AdhocMount {
    host: String,
    share: String,
    mount_point: PathBuf,
    via: InterfaceType,
}

struct DriveConfig {
    id: DriveId,
    label: String,               // User-friendly name
    server_hostname: String,     // Bonjour name (e.g., "macmini.local")
    server_ethernet_ip: Option<IpAddr>, // Preferred ethernet IP
    share_name: String,          // SMB share name
    username: String,            // Keychain lookup key
    mount_point: PathBuf,        // /Volumes/ShareName
    enabled: bool,
}

enum DriveStatus {
    Disconnected,
    Mounting,
    Connected { via: InterfaceType, ip: IpAddr },
    Reconnecting { from: InterfaceType, to: InterfaceType },
    Error(String),
}

enum InterfaceType { Ethernet, WiFi }
```

AppState is registered as a GPUI `Global` so all components can access it via `cx.global::<AppState>()`.

### Event Flow

1. **Network change** → `monitor.rs` detects via SCDynamicStore → sends message to GPUI main thread
2. **GPUI receives event** → calls `mount::manager::reconcile_all(cx)`
3. **Reconcile logic** per drive:
   - Get current `DriveStatus` and available interfaces
   - If disconnected + interface available → mount via best interface
   - If connected via WiFi + Ethernet now available → unmount WiFi, remount via Ethernet
   - If connected via Ethernet + Ethernet went down → unmount, remount via WiFi
   - If no interface available → mark disconnected
4. **Status change** → update `AppState` → tray menu rebuilds → GPUI windows re-render

### Mount Decision Algorithm

```
fn reconcile_drive(drive: &DriveConfig, status: &DriveStatus, interfaces: &[NetworkInterface]):
    let ethernet_up = interfaces.iter().find(|i| i.type == Ethernet && i.has_ip)
    let wifi_up = interfaces.iter().find(|i| i.type == WiFi && i.has_ip)

    let best_target = match (ethernet_up, wifi_up):
        (Some(eth), _) => Some((Ethernet, drive.server_ethernet_ip.unwrap_or(resolve(drive.server_hostname))))
        (None, Some(wifi)) => Some((WiFi, resolve(drive.server_hostname)))
        (None, None) => None

    match (status, best_target):
        (Disconnected, Some(target)) => mount(drive, target)
        (Connected { via: WiFi }, Some((Ethernet, ip))) => remount(drive, Ethernet, ip)
        (Connected { via: Ethernet }, None) if wifi_up => remount(drive, WiFi, resolve(...))
        (Connected { via }, None) => force_unmount(drive); mark_disconnected()
        (Connected { via }, Some((same_type, _))) if via == same_type => no-op
        _ => no-op
```

### Tray Icon Integration (timing-sensitive)

```rust
// main.rs
fn main() {
    Application::new().run(|cx: &mut App| {
        // 1. Initialize AppState global
        cx.set_global(AppState::new());

        // 2. Load config from disk
        config::store::load(cx);

        // 3. Defer tray creation (macOS requires event loop to be running)
        cx.defer(|cx| {
            tray::install(cx);  // Creates TrayIcon via TrayIconBuilder
        });

        // 4. Spawn network monitor on background thread
        cx.spawn(|cx| async move {
            network::monitor::start(cx).await;
        }).detach();

        // 5. Auto-connect configured drives
        cx.defer(|cx| {
            mount::manager::reconcile_all(cx);
        });
    });
}
```

### Menu Event Bridge

```rust
// tray.rs — poll MenuEvent from a repeating GPUI timer
fn start_event_polling(cx: &mut App) {
    cx.spawn(|mut cx| async move {
        loop {
            // Sleep briefly to avoid busy-waiting
            cx.background_executor().timer(Duration::from_millis(100)).await;

            cx.update(|cx| {
                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    handle_menu_event(event, cx);
                }
                while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                    handle_tray_event(event, cx);
                }
            }).ok();
        }
    }).detach();
}
```

## Click Behavior Strategy

**MVP (Phases 1-8):** Left-click the menu bar icon → **native dropdown menu** showing drive statuses and quick actions. "Scan Network...", "Preferences..." menu items open GPUI windows. This is the standard macOS pattern, reliable, and simple to implement.

**Future enhancement (Phase 9+):** Replace the native menu with a **GPUI popover panel** anchored near the menu bar icon — a richer dashboard (like iStat Menus) showing connection badges, interface speeds, and inline controls. This requires AppKit position queries via `tray_icon.ns_status_item()` + `objc2` to get the status item's screen bounds.

## CLI Interface (AI-Agent Friendly)

The same `mountaineer` binary serves both GUI and CLI modes. No subcommand → launch GUI. Any subcommand → CLI mode (no GUI, outputs to stdout, exits).

```
mountaineer                        # Launch GUI (tray icon + GPUI)
mountaineer status                 # JSON: all managed drives + statuses
mountaineer status --drive "SharedData"  # JSON: single drive status
mountaineer scan                   # JSON: discover hosts + shares on network
mountaineer mount <host> <share>   # Ad-hoc mount a share
mountaineer unmount <mount_point>  # Unmount a share
mountaineer drives list            # JSON: all configured managed drives
mountaineer drives add --hostname macmini.local --share SharedData --username myuser
mountaineer drives remove <drive_id_or_label>
mountaineer drives enable <drive_id_or_label>
mountaineer drives disable <drive_id_or_label>
mountaineer reconnect              # Force reconcile all managed drives
mountaineer reconnect <drive_id_or_label>  # Reconcile one drive
mountaineer interfaces             # JSON: current network interfaces + state
```

### Dual-Mode Architecture

```
mountaineer [no args]  →  Launch GPUI app + start IPC server on /tmp/mountaineer.sock
mountaineer <command>  →  If GUI running: send command via IPC, print response
                          If GUI not running: execute command standalone (no GPUI)
```

- **All CLI output is JSON** by default (machine-readable for AI agents)
- `--human` flag available for pretty-printed table output
- Exit codes: 0 = success, 1 = error (with JSON error detail)
- IPC protocol: newline-delimited JSON over Unix domain socket
- CLI commands that modify state (mount, unmount, drives add/remove) trigger AppState updates in the GUI if running

### CLI Entry Point

```rust
// main.rs
fn main() {
    let cli = Cli::parse();

    match cli.command {
        None => launch_gui(),           // No subcommand → GUI mode
        Some(cmd) => {
            // Try IPC to running GUI first
            if let Ok(response) = ipc::send_command(&cmd) {
                println!("{}", response);
            } else {
                // No GUI running, execute standalone
                let result = cli::commands::execute(cmd);
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
    }
}
```

## Implementation Phases

### Phase 1: Skeleton + Tray Icon (get something running)
- `cargo init`, add dependencies
- Minimal `main.rs`: GPUI Application::new().run + App::defer → tray-icon
- Placeholder menu: "Mountaineer" title, "Quit" item
- LSUIElement via cargo-bundle
- **Verify:** app appears as menu bar icon, no dock icon, Quit works

#### Draft Epics & Tasks

**Epic: [Scaffold] Cargo project + GPUI entry point**
**Goal:** Init project with dependencies, create minimal `main.rs` with `Application::new().run`
- Task: `cargo init`, add gpui/tray-icon/log/env_logger deps
- Task: Create `main.rs` with GPUI app bootstrap

**Epic: [Scaffold] Tray icon + placeholder menu**
**Goal:** Show a tray icon in macOS menu bar with "Mountaineer" title and working Quit
- Task: Create `tray.rs` — TrayIconBuilder + placeholder menu
- Task: Wire menu event polling loop
- Task: Wire Quit item to `cx.quit()`

**Epic: [Scaffold] App bundle config (no dock icon)**
**Goal:** Configure LSUIElement and app icon so the app runs as menu-bar-only
- Task: Create `Info.plist.ext` with `LSUIElement = true`
- Task: Add icon assets, configure cargo-bundle metadata

---

### Phase 2: Network Interface Detection
- `src/network/interface.rs`: enumerate interfaces, classify as Ethernet/WiFi
- `src/network/monitor.rs`: SCDynamicStore listener for network changes
- Bridge events to GPUI main thread
- **Verify:** log messages when plugging/unplugging Ethernet

#### Draft Epics & Tasks

**Epic: [Network] Interface enumeration**
**Goal:** Enumerate active network interfaces, classify as Ethernet/WiFi, get IPs
- Task: Define `NetworkInterface` struct + `InterfaceType` enum
- Task: Implement `interface.rs` — enumerate via `system-configuration`, classify, get IPs

**Epic: [Network] SCDynamicStore change monitor**
**Goal:** Detect network changes in real-time and bridge events to GPUI main thread
- Task: Implement `monitor.rs` — SCDynamicStore listener on background thread
- Task: Bridge change events to GPUI main thread via channel
- Task: Log interface up/down transitions

---

### Phase 3: Mount Manager (core value)
- `src/mount/smb.rs`: wrappers around `mount_smbfs` and `diskutil unmount force`
- `src/mount/manager.rs`: reconcile algorithm
- `src/app_state.rs`: AppState with drive configs and statuses
- Hardcode one test drive for development
- **Verify:** drive auto-mounts on launch, reconnects when switching interfaces

#### Draft Epics & Tasks

**Epic: [Mount] AppState global**
**Goal:** Create central AppState with drive configs, statuses, and interfaces as a GPUI Global
- Task: Define `DriveConfig`, `DriveStatus`, `InterfaceType` structs/enums
- Task: Create `app_state.rs` — register AppState as GPUI Global

**Epic: [Mount] SMB mount/unmount shell wrappers**
**Goal:** Wrap `mount_smbfs` and `diskutil unmount force` as callable Rust functions
- Task: Implement `smb::mount()` — shell out to `mount_smbfs`
- Task: Implement `smb::unmount()` — shell out to `diskutil unmount force`

**Epic: [Mount] Reconcile engine (failover/failback)**
**Goal:** Automatically mount drives on best interface and switch on network changes
- Task: Implement `reconcile_drive()` — best-interface selection logic
- Task: Implement `reconcile_all()` — iterate all drives, call reconcile_drive
- Task: Wire network monitor events to trigger `reconcile_all`
- Task: Hardcode one test drive, verify mount/failover/failback cycle

---

### Phase 4: Config Persistence + Keychain
- `src/config/store.rs`: TOML config at `~/.config/mountaineer/config.toml`
- `src/keychain.rs`: store/retrieve credentials via `security-framework`
- Config includes drive definitions, preferences
- **Verify:** drive configs persist across restarts, credentials stored securely

#### Draft Epics & Tasks

**Epic: [Config] TOML config load/save**
**Goal:** Persist drive definitions and preferences to `~/.config/mountaineer/config.toml`
- Task: Define `Config` / `GeneralConfig` structs with serde derives
- Task: Implement `config::store::load()` and `config::store::save()`
- Task: Load config on startup, populate AppState

**Epic: [Keychain] Credential store/retrieve**
**Goal:** Store and retrieve SMB passwords from macOS Keychain
- Task: Implement `keychain.rs` — store/retrieve via `security-framework`
- Task: Integrate Keychain lookup into `smb.rs` mount flow

---

### Phase 4b: CLI + IPC Layer
- `src/cli/mod.rs`: Clap derive CLI with subcommands (status, scan, mount, unmount, drives, reconnect, interfaces)
- `src/cli/commands.rs`: standalone command execution (reads config, calls mount/smb directly)
- `src/cli/ipc.rs`: Unix socket client — connect to `/tmp/mountaineer.sock`, send JSON command, read JSON response
- `src/ipc_server.rs`: Unix socket server — spawned in GUI mode, receives commands, dispatches to AppState/mount manager, returns JSON
- All commands work both standalone (no GUI) and via IPC (GUI running)
- **Verify:** `mountaineer status` returns JSON; `mountaineer drives list` shows configured drives; `mountaineer scan` discovers hosts; `mountaineer mount macmini.local SharedData` mounts a share

#### Draft Epics & Tasks

**Epic: [CLI] Clap subcommand definitions**
**Goal:** Define all CLI subcommands with Clap derive (status, scan, mount, unmount, drives, reconnect, interfaces)
- Task: Define Clap structs in `cli/mod.rs`
- Task: Update `main.rs` — no subcommand → GUI, subcommand → CLI

**Epic: [CLI] Standalone command handlers**
**Goal:** Implement command handlers that work without the GUI (read config, call mount/smb directly)
- Task: Implement handlers in `cli/commands.rs` for each subcommand
- Task: JSON output by default, `--human` flag for tables

**Epic: [IPC] Unix socket server (GUI side)**
**Goal:** GUI listens on `/tmp/mountaineer.sock`, receives CLI commands, dispatches to AppState
- Task: Create `ipc_server.rs` — listen, parse JSON, dispatch, return response
- Task: Spawn server on GUI startup

**Epic: [IPC] Unix socket client (CLI side)**
**Goal:** CLI connects to running GUI via socket, falls back to standalone if no GUI
- Task: Create `cli/ipc.rs` — connect, send command, read response
- Task: Wire auto-detection: try IPC first, fall back to standalone

---

### Phase 5: Dynamic Tray Menu
- `src/tray.rs`: rebuild menu from AppState on every status change
- Drive status section (per managed drive):
  - "SharedData — Connected (Ethernet)" / "TimeMachine — Disconnected"
  - Clicking a connected drive → "Open in Finder" (open mount point)
- Separator + action items:
  - "Reconnect All" — force reconcile all managed drives
  - "Browse Network..." — opens Network Browser GPUI window (Phase 7a)
  - "Drive Dashboard..." — opens dashboard GPUI window (Phase 7b)
  - "Preferences..." — opens settings GPUI window (Phase 7c)
  - Separator
  - "Quit Mountaineer"
- Color-coded tray icon based on aggregate status:
  - Green: all enabled drives connected
  - Yellow: some connected, some disconnected/reconnecting
  - Red/gray: no drives connected
  - Idle: no drives configured
- **Verify:** menu updates live as drives connect/disconnect/switch

#### Draft Epics & Tasks

**Epic: [Tray] Dynamic drive status menu**
**Goal:** Rebuild tray menu from AppState on every change, showing per-drive status rows
- Task: Build menu dynamically from `AppState.drives` + `drive_statuses`
- Task: Per-drive rows: "Label — Status (Interface)"
- Task: Click connected drive → Open in Finder

**Epic: [Tray] Action menu items**
**Goal:** Add Reconnect All, Browse Network, Dashboard, Preferences, Quit to tray menu
- Task: Add action items with separators
- Task: Wire menu events to open GPUI windows / trigger reconcile

**Epic: [Tray] Color-coded status icon**
**Goal:** Swap tray icon (green/yellow/red/idle) based on aggregate drive health
- Task: Implement aggregate status calculation
- Task: Swap icon on every AppState change

---

### Phase 6: Network Discovery + Share Enumeration
- `src/network/scanner.rs`: two-stage discovery:
  1. **Host discovery**: mDNS browse for `_smb._tcp.local.` services → populates `DiscoveredHost` list
  2. **Share enumeration**: for each discovered host, run `smbutil view //guest@hostname` to list available shares → populates `DiscoveredShare` per host
- Cross-reference with managed drives: mark shares that are already in `drives[]` config as `is_managed = true`
- Cross-reference with current mounts: check `/Volumes/` or `mount` output to mark `is_mounted = true`
- Store results in `AppState.discovered_hosts`
- Scanner runs on-demand (user clicks "Scan Network") and optionally on a slow interval (e.g. every 60s if configured)
- **Verify:** discovers Mac Mini and other SMB hosts on LAN, lists their shares

#### Draft Epics & Tasks

**Epic: [Discovery] mDNS host discovery**
**Goal:** Browse `_smb._tcp.local.` via mdns-sd to find SMB hosts on the LAN
- Task: Implement mDNS browse → `DiscoveredHost` list
- Task: Store results in `AppState.discovered_hosts`
- Task: On-demand scan trigger + optional periodic interval

**Epic: [Discovery] Share enumeration via smbutil**
**Goal:** For each discovered host, run `smbutil view` to list available shares
- Task: Implement `smbutil view` output parser
- Task: Run enumeration per host on background thread (5s timeout)
- Task: Cross-reference shares with managed drives + current mounts (is_managed, is_mounted)

---

### Phase 7: GPUI UI — Network Browser + Drive Management
Three GPUI windows, openable from tray menu:

**7a. Network Browser** (`src/ui/scanner.rs`)
- Tree view: Computer → Shares
- Each computer row: hostname, IP, number of shares
- Each share row: share name, status badge (Mounted/Managed/Available), action buttons:
  - **"Mount"** — one-click ad-hoc mount (prompts for credentials if not in Keychain, mounts to `/Volumes/ShareName`)
  - **"Add to Mountaineer"** — adds to managed drives config (opens mini-form: label, mount point, auto-connect toggle) → saves to config.toml + Keychain
  - **"Unmount"** — if currently mounted (ad-hoc or managed)
  - **"Open in Finder"** — if mounted, opens the mount point in Finder
- "Scan Now" button in header, spinner while scanning
- Auto-refreshes when scanner completes

**7b. Drive Dashboard** (`src/ui/drive_list.rs`)
- List of all managed drives with live status:
  - Drive label, share path, current status (Connected/Disconnected/Error)
  - Connected via badge: "Ethernet" (green) / "WiFi" (yellow)
  - IP address currently connected through
- Per-drive actions:
  - **"Reconnect"** — force unmount + remount via best interface
  - **"Unmount"** — disconnect this drive
  - **"Remove"** — remove from managed drives (confirms, unmounts, removes from config.toml + Keychain)
  - **"Open in Finder"** — open mount point
- Bulk actions: "Reconnect All", "Unmount All"

**7c. Settings** (`src/ui/settings.rs`)
- General preferences: prefer Ethernet, check interval, notifications, launch at login
- Drive list: edit any drive's config (label, hostname, ethernet IP, share, username, mount point)
- "Add Drive Manually" form for drives not discoverable via mDNS
- Keychain management: re-enter credentials for a drive

- **Verify:** can discover a share in Network Browser → one-click Mount → one-click "Add to Mountaineer" → see it in Drive Dashboard → Remove it

#### Draft Epics & Tasks

**Epic: [UI] Network Browser — host/share tree view**
**Goal:** GPUI window showing discovered hosts and their shares with status badges
- Task: Create window shell, wire to tray menu "Browse Network..."
- Task: Render host rows (hostname, IP, share count)
- Task: Render expandable share rows with status badges (Mounted/Managed/Available)
- Task: "Scan Now" button + spinner

**Epic: [UI] Network Browser — share actions**
**Goal:** One-click Mount, Add to Mountaineer, Unmount, Open in Finder per share
- Task: "Mount" button — ad-hoc mount with credential prompt
- Task: "Add to Mountaineer" — mini-form, save to config + Keychain
- Task: "Unmount" and "Open in Finder" buttons

**Epic: [UI] Drive Dashboard — status list**
**Goal:** GPUI window listing managed drives with live status and interface badge
- Task: Create window shell, wire to tray menu "Drive Dashboard..."
- Task: Render drive list: label, share path, status, interface badge, IP
- Task: Live-update on AppState changes

**Epic: [UI] Drive Dashboard — drive actions**
**Goal:** Per-drive Reconnect/Unmount/Remove/Open + bulk actions
- Task: Implement Reconnect, Unmount, Open in Finder per drive
- Task: Implement Remove — confirm dialog, unmount, clean config + Keychain
- Task: Bulk Reconnect All / Unmount All bar

**Epic: [UI] Settings — general preferences**
**Goal:** GPUI window for editing prefer Ethernet, check interval, notifications, launch at login
- Task: Create window shell, wire to tray menu "Preferences..."
- Task: Implement toggle/input controls, save to config

**Epic: [UI] Settings — drive config editor**
**Goal:** Edit existing drive fields + add drives manually
- Task: Drive list editor — edit label, hostname, IP, share, username, mount point
- Task: "Add Drive Manually" form

**Epic: [UI] Settings — Keychain management**
**Goal:** Re-enter or update credentials per drive from Settings
- Task: Credential re-entry form per drive
- Task: Save updated credentials to Keychain

---

### Phase 8: Launch at Login
- `launchd/com.mountaineer.agent.plist`: LaunchAgent pointing to .app bundle
- Install/uninstall from Settings UI
- **Verify:** app starts on login, stays running

#### Draft Epics & Tasks

**Epic: [System] LaunchAgent plist + install/uninstall**
**Goal:** Create LaunchAgent and wire Settings toggle so app starts on login
- Task: Create `com.mountaineer.agent.plist` template
- Task: Implement install (copy to `~/Library/LaunchAgents/`) and uninstall (remove)
- Task: Wire to Settings UI toggle

---

## Config File Format

`~/.config/mountaineer/config.toml`:

```toml
[general]
check_interval_secs = 5
prefer_ethernet = true
notifications = true

[[drives]]
label = "Mac Mini Storage"
hostname = "macmini.local"
ethernet_ip = "192.168.1.100"
share = "SharedData"
username = "myuser"
mount_point = "/Volumes/SharedData"
enabled = true
```

## Cargo.toml Dependencies

```toml
[package]
name = "mountaineer"
version = "0.1.0"
edition = "2021"

[dependencies]
gpui = "0.2"
tray-icon = "0.21"
system-configuration = "0.6"
mdns-sd = "0.12"
security-framework = "3"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
log = "0.4"
env_logger = "0.11"
dirs = "6"
uuid = { version = "1", features = ["v4"] }
clap = { version = "4", features = ["derive"] }
serde_json = "1"

[package.metadata.bundle]
name = "Mountaineer"
identifier = "com.mountaineer.app"
icon = ["assets/icon-connected.png"]
osx_info_plist_exts = ["assets/Info.plist.ext"]
```

## Verification Plan

| Phase | Test |
|---|---|
| 1 | App appears in menu bar, no dock icon, Quit works |
| 2 | Console logs interface changes on plug/unplug Ethernet |
| 3 | Hardcoded share mounts on launch; unplug Ethernet → remounts via WiFi; plug back in → switches to Ethernet |
| 4 | Kill and relaunch app → same drives configured; `security find-internet-password` shows stored cred |
| 4b | `mountaineer status` → JSON output; `mountaineer scan` → discovers hosts+shares; `mountaineer mount host share` → mounts; with GUI running, CLI commands go via IPC |
| 5 | Tray menu shows live drive status; icon color changes; "Browse Network..." and "Drive Dashboard..." open windows |
| 6 | "Scan Network" finds SMB hosts AND lists their individual shares; `smbutil view` output parsed correctly |
| 7a | Network Browser: see computers + shares, one-click Mount works, "Add to Mountaineer" saves to config, "Unmount" works, "Open in Finder" works |
| 7b | Drive Dashboard: see all managed drives with live status, Reconnect/Unmount/Remove per drive work, Remove cleans config + Keychain |
| 7c | Settings: edit drive config, add drive manually, toggle launch at login |
| 8 | Log out / log in → app starts automatically |

## Risk Mitigations

- **GPUI is pre-1.0**: Pin to exact version in Cargo.toml. If GPUI has blockers, the tray/network/mount logic is framework-agnostic and can be rewired to egui later.
- **tray-icon timing on macOS**: Use `App::defer` as primary strategy; fall back to delayed spawn if needed.
- **mount_smbfs requires credentials**: Use `-N` flag with Keychain-stored credentials, or pass via URL (`//user:pass@host/share`). Prefer Keychain.
- **Force unmount may fail**: Retry with `diskutil unmount force`, then `umount -f` as last resort. Log failures.
- **SCDynamicStore thread safety**: Run on dedicated background thread, bridge to GPUI main thread via channel.
- **`smbutil view` may require credentials**: Try guest first (`//guest@host`), then prompt for credentials if it fails. Cache auth results in Keychain.
- **`smbutil view` can be slow/timeout**: Run on background thread with a 5s timeout per host. Show spinner in UI. Cancel previous scan if user starts a new one.
- **Ad-hoc mounts have no failover**: Only managed drives get automatic interface switching. Ad-hoc mounts are "mount it now" convenience — if the connection drops, the user re-mounts from the browser. This keeps the model simple.
