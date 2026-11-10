# Changelog

## [Unreleased]

### Removed

- `--cookie` and `RBX_COOKIE`. The value was sent as the whole `Cookie` header, without `.ROBLOSECURITY=`, so it authenticated nothing, and the `--universe-id` listing it was for answers without a credential.

### Fixed

- Under WSL a place file is opened with `Invoke-Item -LiteralPath`, which reads its name as it is, so `[`, `]` and a backtick in it are part of the name. `Start-Process` read the path as a wildcard pattern, under which `place[1].rbxl` resolves to a `place1.rbxl` beside it. `Start-Process` is kept for links.
- A launcher that fails is an error, with a non-zero exit, instead of a success line. macOS waits for `open` and reads its exit code; on Linux and WSL a route that exits non-zero counts as one that did not work, so the next is tried. On Windows a failure to start `explorer.exe` is reported; its exit code is not read, so a missing Studio goes unnoticed there.
- Under WSL a program that started and failed is not started a second time under its full `/mnt/c` path: that path is tried only when the bare name is not found.
- Under WSL `rundll32.exe` is the last route tried, after PowerShell, since it exits 0 whether or not anything opened the place. When it is the one that took it, the success line says the place was handed to Windows, not that it opened.
- When nothing on Linux or WSL could open the place, the error names what it was trying to reach (Studio, the Roblox Player for `--play`, or the file) and lists each program tried and how it ended (its exit status, "not found", or why it could not start).
- On Windows a place file on a network share is handed over as `\\server\share\dir\place.rbxl`, not `UNC\server\share\dir\place.rbxl`, a relative path that named nothing.

### Security

- On Windows the place is handed to `explorer.exe` by its full path in the Windows directory, with no shell in between, so an `explorer.bat` or `explorer.exe` in the current directory or beside `rbx-open.exe` is never run in its place. The directory comes from `SystemRoot`, or `WINDIR` when `SystemRoot` is missing or not absolute; with neither usable, the launch is an error naming both. macOS starts `/usr/bin/open` the same way.
- Under WSL the target reaches PowerShell through an environment variable shared with `WSLENV`, never inside the command PowerShell parses. A file name holding a quote PowerShell closes a string on (`'`, or U+2018 to U+201B) can no longer run as code. The user's other `WSLENV` entries are kept byte for byte, including ones that are not UTF-8; an existing entry for `RBX_OPEN_TARGET` is replaced, flags and all.
- Under WSL no route goes through a shell or a script. `cmd.exe` read `&`, `|`, `^` and `%` in a file name as its own syntax, and `wslview`, a shell script that builds its own Windows command line, is no longer tried. PowerShell and `rundll32.exe` remain.
- Release archives carry a build provenance attestation: `gh attestation verify <zip> --repo rbx-forge/rbx-open`. The build jobs run with read access only, and the publishing job alone may write.

## [0.1.0] - 2026-11-08

First release as a standalone tool.

- Opens a place in Studio by its name in `rbxplace.toml`, by `--place-id`, through a `--universe-id` listing, or from a `.rbxl` / `.rbxlx` file.
- `--new` opens a new place from a template, picked from Roblox's list or named with `--baseplate` / `--template`.
- `--play` joins the place in the Roblox client instead.
- `--places <path>` reads `rbxplace.toml` from somewhere other than the current directory.
