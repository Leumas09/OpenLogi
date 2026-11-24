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

See the sections below; each lists the files touched so a conflicting merge
is easy to resolve.
