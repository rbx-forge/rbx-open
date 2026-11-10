# Changelog

## [Unreleased]

### Removed

- `--cookie` and `RBX_COOKIE`. The value was sent as the whole `Cookie` header, without `.ROBLOSECURITY=`, so it authenticated nothing, and the `--universe-id` listing it was for answers without a credential.

### Fixed

- Under WSL a place file whose name holds `[`, `]`, `*`, `?` or a backtick opens that file, not another one its name matches as a pattern (`place[1].rbxl` opened `place1.rbxl`). The path is escaped before `Start-Process` reads it; a link is passed unescaped.
- A launcher that fails is an error, with a non-zero exit, instead of a success line. macOS waits for `open` and reads its exit code; on Linux and WSL a route that exits non-zero counts as one that did not work, so the next is tried. On Windows a failure to start `explorer.exe` is reported; its exit code is not read, since it exits 1 after a hand-off that worked.
- Under WSL a program that started and failed is not started a second time under its full `/mnt/c` path: that path is tried only when the bare name is not found.
- When nothing on Linux or WSL could open the place, the error lists each program tried and how it ended (its exit status, "not found", or why it could not start).
- A place file on a network share opens on Windows: Studio gets `\\server\share\dir\place.rbxl`, not `UNC\server\share\dir\place.rbxl`.

### Security

- On Windows the place is handed to `explorer.exe` with no shell in between, so an `explorer.bat` or `explorer.exe` in the current directory is never run in its place. It is started by its full path in the Windows directory (`SystemRoot`), or by its bare name when `SystemRoot` is missing or not absolute, a lookup that never searches the current directory. macOS starts `/usr/bin/open` the same way.
- Under WSL the target reaches PowerShell through an environment variable shared with `WSLENV`, never inside the command PowerShell parses. A file name holding a quote PowerShell closes a string on (`'`, or U+2018 to U+201B) can no longer run as code. The user's own `WSLENV` is kept as it is, including a value that is not UTF-8.
- Under WSL no route goes through `cmd.exe`, which read `&`, `|`, `^` and `%` in a file name as its own syntax. PowerShell, `rundll32.exe` and `wslview` remain.
- Release archives carry a build provenance attestation: `gh attestation verify <zip> --repo rbx-forge/rbx-open`. The build jobs run with read access only, and the publishing job alone may write.

## [0.1.0] - 2026-11-08

First release as a standalone tool.

- Opens a place in Studio by its name in `rbxplace.toml`, by `--place-id`, through a `--universe-id` listing, or from a `.rbxl` / `.rbxlx` file.
- `--new` opens a new place from a template, picked from Roblox's list or named with `--baseplate` / `--template`.
- `--play` joins the place in the Roblox client instead.
- `--places <path>` reads `rbxplace.toml` from somewhere other than the current directory.
