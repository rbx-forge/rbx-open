# Changelog

## [Unreleased]

First release as a standalone tool.

- Opens a place in Studio by its name in `rbxplace.toml`, by `--place-id`, through a `--universe-id` listing, or from a `.rbxl` / `.rbxlx` file.
- `--new` opens a new place from a template, picked from Roblox's list or named with `--baseplate` / `--template`.
- `--play` joins the place in the Roblox client instead.
- `--places <path>` reads `rbxplace.toml` from somewhere other than the current directory.
