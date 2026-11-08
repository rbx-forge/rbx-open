# rbx-open

Open a Roblox place in Studio, or join it in the client, from the command line: by its name in `rbxplace.toml`, by id, from a file on disk, or as a new place from a template.

## Requirements

Roblox Studio installed on the machine (or the Roblox app, for `--play`), on Windows or macOS. On Linux the link goes to `xdg-open`, so it works wherever something has registered the `roblox-studio:` scheme, and under WSL it reaches Studio on the Windows side.

## Install

With [mise](https://mise.jdx.dev) or [Rokit](https://github.com/rojo-rbx/rokit), either of which pins the latest release in your project's config:

```bash
mise use --pin github:rbx-forge/rbx-open
rokit add rbx-forge/rbx-open   # or, with Rokit
```

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
- `--cookie <value>`: `.ROBLOSECURITY` for the `--universe-id` listing, or `RBX_COOKIE`; that listing answers without one

## Playing instead of editing

`--play` changes the app and nothing else: the same `<env> <place>`, `--place-id` and `--universe-id` name the place, and there is still no network call. It is refused with `--new` and with a file on disk, which name a place that is not published and so has nothing to join.

The link sent is `roblox://experiences/start?placeId=<id>`, the deep link the Roblox client handles itself, rather than the `roblox-player:1+launchmode:play+...` the website sends. The website's form carries `gameinfo:<ticket>`, a single-use authentication ticket issued for one account; the same URI without it was measured on Windows starting the client and stopping at its home screen, with nothing to join.

**It does not choose an account.** The place is joined as whoever is signed into the Roblox app.

## A file on disk

Recognised by extension, not by looking on disk: `rbx-open prod` has to stay an environment even in a folder that happens to contain a file called `prod`.

The path is handed to the desktop's opener rather than wrapped in a `roblox-studio:` URI, because that URI is parsed by splitting on `+` and `:`, which both a Windows path and any filename containing a `+` would break. Studio ends up in the same place either way.

## A new place

`--new` is Studio's "New Experience" button: an ordinary `roblox-studio:` open of Roblox's stock baseplate, which is exactly what `--new --baseplate` sends.

**Nothing is created on Roblox.** Studio binds the session to the template long enough to fetch its content, then sets the place id back to `0`. The content arrives, the identity does not, so the first "Save to Roblox As" or "Publish to Roblox As" is what creates the experience. DataStores do not work until then: there is no universe to address.

`--new` on its own lists Roblox's templates and asks. There is no template API: the list is the public games of the account Studio reads it from too, and it needs no credential. `--baseplate` and `--template` skip the picker, so they work without a terminal and without the network call.

## When a place opens as an empty, unsaved one

Studio opens a place you are not allowed to edit as a detached copy: the window looks right, but the session has no place id and a save would create a new experience. The account signed into Studio has no edit rights on that place. Sign in with one that has them and open it again.

## How it works

```
roblox-studio:1+task:EditPlace+placeId:123456789+universeId:0
```

That URI is handed to your system's Roblox Studio installation. On Windows it goes through `explorer`, and the tool waits for the hand-off, so a launcher that tears down its child processes on exit cannot kill it halfway. On macOS it goes to `open`. On Linux it goes to `xdg-open`, and if that is not there, or under WSL where Studio lives on the Windows side, it crosses to the Windows host through `powershell.exe Start-Process`, then `rundll32`, then `cmd.exe start`, then `wslview`.

## Prior art

[ROpen](https://github.com/Barocena/ROpen) (Luau, MPL-2.0) is where this tool comes from: the launcher it was written against, and where the `roblox-studio:` URI dispatch was learned from, having contributed to it.

That dispatch is older than either. [rojo-rbx/edit-roblox-place](https://github.com/rojo-rbx/edit-roblox-place) (Rust, MIT) was doing the same thing in **August 2019**: one command, one place id, and the same `roblox-studio:1+task:EditPlace+placeId:<id>` this tool sends.

What this tool adds is that the place is named rather than numbered. No code from either project is reused.

## License

[MPL-2.0](LICENSE)
