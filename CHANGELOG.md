# Changelog

## [Unreleased]

### Security

- Release archives carry a build provenance attestation: `gh attestation verify <zip> --repo rbx-forge/rbx-open`. The build jobs run with read access only, and the publishing job alone may write.

## [0.1.0] - 2026-11-08

First release as a standalone tool.

- Opens a place in Studio by its name in `rbxplace.toml`, by `--place-id`, through a `--universe-id` listing, or from a `.rbxl` / `.rbxlx` file.
- `--new` opens a new place from a template, picked from Roblox's list or named with `--baseplate` / `--template`.
- `--play` joins the place in the Roblox client instead.
- `--places <path>` reads `rbxplace.toml` from somewhere other than the current directory.
