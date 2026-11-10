# rbx-open

Open a Roblox place in Studio, or join it in the client, from the command line: by its name in `rbxplace.toml`, by id, from a file on disk, or as a new place from a template.

## Requirements

Roblox Studio installed on the machine (or the Roblox app, for `--play`), on Windows or macOS. On Linux the link goes to `xdg-open`, so it works wherever something has registered the `roblox-studio:` scheme, and under WSL it reaches Studio on the Windows side.

## Install

With [mise](https://mise.jdx.dev):

```bash
mise use --pin github:rbx-forge/rbx-open
```

Or with [Rokit](https://github.com/rojo-rbx/rokit):

```bash
rokit add rbx-forge/rbx-open
```

Either one pins the latest release in your project's config.

Or take a binary for your platform from the [releases page](https://github.com/rbx-forge/rbx-open/releases), which ships a `SHA256SUMS` alongside the archives, or build from source:

```bash
cargo install --git https://github.com/rbx-forge/rbx-open --locked
```

## Quick start

```bash
rbx-open prod main            # a place named in rbxplace.toml
rbx-open staging              # the only place in an env
rbx-open                      # pick an env, then a place
rbx-open --place-id 123456789 # by id, no file needed
rbx-open game.rbxl            # a place file on disk
rbx-open --new                # a new place from a template
rbx-open prod main --play     # join it in the Roblox client instead
```

## Naming places: `rbxplace.toml`

Names are an option, not a requirement: ids, files and `--new` work in any directory. When a project has an `rbxplace.toml`, each top-level table is an environment and its `places` map names to ids:

```toml
[prod]
universe_id = 9876543210
places.main = 123456789012345
places.lobby = 987654321

[dev]
universe_id = 9876543212
places.main = 345678901234567
```

`rbx-open <env> <place>` resolves through it. An env with one place needs no place name, and a missing argument becomes a picker. `--places <path>` reads a file stored somewhere else.

## Options

- `<ENV> [PLACE]`: names from `rbxplace.toml`, or a `.rbxl` / `.rbxlx` path as the first argument
- `--places <path>`: where `rbxplace.toml` is (default: `./rbxplace.toml`)
- `--place-id <id>`: a place by id, no file needed; wins over `<env> <place>`
- `--universe-id <id>`: list a universe's places from Roblox and open one; a single-place universe opens without asking
- `--file <path>`: a place file whose name has no `.rbxl` / `.rbxlx` extension
- `--new`: a new place from a template; `--baseplate` takes the stock one, `--template <id>` names one
- `--play`: join the place in the Roblox client instead of opening it in Studio

No option takes a credential: the `--universe-id` listing and the template list answer without one.

## Playing instead of editing

`--play` opens the same place in the Roblox client, through the `roblox://experiences/start?placeId=<id>` deep link. It joins as whoever is signed into the Roblox app, and is refused with `--new` or a file, which have nothing published to join.

## A new place

`--new` is Studio's "New Experience": without a flag it lists Roblox's templates (no credential needed); `--baseplate` or `--template <id>` skip the list. Nothing is created on Roblox until you "Publish to Roblox As" in Studio, so data stores do not work before that.

## A place that opens empty and unsaved

The signed-in Studio account has no edit rights on that place, and Studio opened a detached copy: saving it would create a new experience. Sign in with an account that can edit it and open it again.

## How it works

The tool hands Studio a `roblox-studio:1+task:EditPlace+placeId:<id>` link: through `explorer.exe` on Windows, `open` on macOS, `xdg-open` on Linux (and the Windows side under WSL). A file on disk is recognised by its extension and passed to the system's opener.

A failure it can see is an error with a non-zero exit, and some failures it cannot see:

- On Windows, `explorer.exe` failing to start is an error, but its exit code is not read: it is 1 after a hand-off that worked too, so a missing Studio goes unnoticed.
- On macOS and Linux, `open` and `xdg-open` report a link or file they cannot open, and that is an error.
- Under WSL, PowerShell is tried first, then `rundll32.exe`. If all of them fail, the error lists each program and how it ended. `rundll32.exe` exits 0 whatever happens, so when it is the one that took the link, the tool says it handed the place to Windows, not that it opened it. How `rundll32.exe` handles a path with spaces is untested.

## Prior art

[ROpen](https://github.com/Barocena/ROpen) (Luau, MPL-2.0), which this tool comes from, and [rojo-rbx/edit-roblox-place](https://github.com/rojo-rbx/edit-roblox-place) (Rust, MIT), which sent the same link in 2019. What this adds is opening places by name. No code from either is reused.
## License

[MPL-2.0](LICENSE). A community tool, not affiliated with Roblox Corporation.
