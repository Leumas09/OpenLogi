# OpenLogi — personal Windows fork

Fork of [AprilNEA/OpenLogi](https://github.com/AprilNEA/OpenLogi) for one
Windows 11 machine (Surface) with an MX Master 3S and an MX Keys over
Bluetooth.

The fork **tracks upstream**: macOS/Linux code and unused features stay in the
tree (they are not compiled on Windows, or cost nothing at runtime), so
upstream merges stay cheap. Every change made here is listed below.

## Remotes and sync

| Remote | URL |
|---|---|
| `origin` | GitHub `Leumas09/OpenLogi` — pushes go to GitHub **and** the NAS |
| `nas` | `ssh://Sam@datastation/volume1/Repo/OpenLogi.git` |
| `upstream` | GitHub `AprilNEA/OpenLogi` (never pushed to) |

```powershell
git fetch upstream
git merge upstream/master
git push
```

## Install

```powershell
.\fork\install.ps1
```

Builds the release binaries and installs them in
`%LOCALAPPDATA%\Programs\OpenLogi`, the official MSI layout. `-NoBuild` reuses
`target\release`.

## Updates

Local builds embed no minisign key, so the built-in updater fails closed and
can never replace this build with an upstream release. The per-launch check
is also switched off in the config (`check_for_updates = false`).

## Windows checkout notes

Without Developer Mode, git checks the repository's symlinks
(`.claude/rules`, `.claude/skills/*`) out as text files. They are replaced
locally by NTFS junctions to `.agents/...` and marked
`git update-index --skip-worktree`.

## Changes from upstream

Each entry lists the files touched, so a conflicting merge is easy to
resolve.

### Indicators above the taskbar (Windows)

The Caps/Num/Scroll/Fn lock and backlight indicators open at the bottom
centre of the monitor under the cursor, above the taskbar, like the
low-battery notices, instead of the middle of the main display. Every bottom
window opens above the ones already showing, so none overlap. Other
platforms are unchanged.

- `crates/openlogi-overlay/src/indicator.rs` (new)
- `crates/openlogi-overlay/src/{main,lock_key,notice}.rs`
- `crates/openlogi-overlay/src/platform/windows.rs`, `.../windows/native.rs`
  (`NoticePlacement::lifted`)

### Fn lock indicator

Fn+Esc on the MX Keys shows "Fn Lock On/Off" like Caps Lock. The agent
decodes the state carried by the keyboard's HID++ `0x40a3`/`0x40a2`
broadcast. `LockKey::FnLock` is appended to the IPC enum; the wire protocol
moves to **v47** (upstream will reuse that number for something else:
on a conflict, keep upstream's version, then bump past it).

- `crates/openlogi-hidpp/src/feature/fn_inversion.rs` (`decode_state_broadcast`)
- `crates/openlogi-device/src/inventory/events.rs`, `src/write.rs`,
  `src/write/fn_lock.rs`
- `crates/openlogi-core/src/device.rs` (`LockKey::FnLock`)
- `crates/openlogi-ipc/src/ipc.rs`, `tests/wire_format.rs`
- `crates/openlogi-overlay/src/lock_key.rs`
- `crates/openlogi-ui/locales/*.toml` (`keyboard.fn_lock_on/off`)
- `crates/openlogi-agent/src/bin/mock_agent/control.rs` (`lock fn on|off`)

### Device photos decoded at display size (RAM)

`img(path)` decoded the depot renders (up to ~2000 px) at full size and
GPUI kept them for the app's lifetime, on the CPU and in the GPU atlas.
`ui::photo::fitted` decodes each photo at its drawn box times the window
scale factor, and keeps at most two sizes per photo.

- `crates/openlogi-desktop/src/ui/photo.rs` (new), `src/ui/mod.rs`
- `crates/openlogi-desktop/src/app/home.rs`, `src/app/home/views.rs`
- `crates/openlogi-desktop/src/features/mouse/view.rs`
- `crates/openlogi-desktop/src/features/keyboard/function_row.rs`

## Memory findings

Measured on this Surface (private working set, as Task Manager shows it):

| | Upstream | Fork |
|---|---|---|
| Idle, window closed (agent + overlay) | ~7.7 MB | ~7.7 MB |
| Window open, first page (MX Keys) | 86–92 MB | 48 MB |
| Window open, after visiting every page | 131–138 MB | 74 MB |

The settings window is its own process and exits when closed, so the
window figures only apply while it is open. What remains with every photo
showing (~35 MB above a photo-less window) is mostly GPUI's texture atlas,
which allocates a full page on the first image.

Things measured and **kept** because removing them saves nothing:

- Webcam support: the 2 s camera scan via Media Foundation costs no
  measurable memory.
- Flow, receivers (Bolt/Unifying), Actions Ring: nothing is loaded until
  used; the overlay process is needed for the indicators anyway.
- macOS/Linux code: not compiled on Windows.
- Agent worker threads (8): trimming them would save ~0.3 MB.
