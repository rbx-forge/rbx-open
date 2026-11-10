# Changelog

## [Unreleased]

### Fixed

- A place file on a network share opens on Windows: Studio gets `\\server\share\dir\place.rbxl`, not `UNC\server\share\dir\place.rbxl`.

### Security

- On Windows the place is handed to `explorer.exe` by its full path in the Windows directory (`SystemRoot`), with no shell in between, so an `explorer.bat` or `explorer.exe` in the current directory is never run in its place. macOS starts `/usr/bin/open` the same way.
- Release archives carry a build provenance attestation: `gh attestation verify <zip> --repo rbx-forge/rbx-open`. The build jobs run with read access only, and the publishing job alone may write.

## [0.1.0] - 2026-11-08

First release as a standalone tool.

- Opens a place in Studio by its name in `rbxplace.toml`, by `--place-id`, through a `--universe-id` listing, or from a `.rbxl` / `.rbxlx` file.
- `--new` opens a new place from a template, picked from Roblox's list or named with `--baseplate` / `--template`.
- `--play` joins the place in the Roblox client instead.
- `--places <path>` reads `rbxplace.toml` from somewhere other than the current directory.
