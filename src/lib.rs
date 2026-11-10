//! Open a Roblox place in Studio, or join it in the client.
//!
//! Resolution order, most direct first:
//! 0. a `.rbxl` / `.rbxlx` path: opened from disk, no id and no network.
//! 1. `--new` : Roblox's baseplate template, the way Studio's own "New
//!    Experience" button opens it. No id to supply, and nothing created.
//! 2. `--place-id <id>` : opened as given, no file and no network.
//! 3. `--universe-id <id>` : the universe's places are listed from Roblox, and
//!    a single-place universe opens without a prompt.
//! 4. `rbxplace.toml`, addressed by the positional `<env> <place>` arguments,
//!    then by a picker.
//!
//! `--play` keeps all of that and changes only the app: the place is joined in
//! the Roblox client instead of opened in Studio. The `place_uri` function
//! carries why the deep link is the form that works, and what it cannot do.

#[cfg(any(target_os = "linux", test))]
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Parser;
use colored::Colorize;
use dialoguer::Select;

use rbx_core::api::ApiBase;
use rbx_core::places::PlacesFile;
use rbx_core::templates::{list_templates, StudioTemplate, DEFAULT_TEMPLATE_PLACE_ID, GAMES_HOST};
use rbx_core::universe::{UniversePlace, DEVELOP_HOST};

#[derive(Parser, Debug)]
#[command(
    name = "rbx-open",
    version,
    about = "Open a Roblox place in Studio, or join it in the client",
    long_about = "Open a Roblox place in Studio, or join it in the client, by its name in \
                  rbxplace.toml, by id, from a file on disk, or as a new place from a template."
)]
pub struct Cli {
    /// Environment name from `rbxplace.toml` (e.g. `prod`, `staging`), or a
    /// `.rbxl` / `.rbxlx` path. Falls back to an interactive picker.
    pub env: Option<String>,

    /// Place name within the env (e.g. `main`, `lobby`). Falls back to an
    /// interactive picker, which picks on its own when the env has exactly one
    /// place.
    pub place: Option<String>,

    /// Open a new, empty place, like Studio's "New Experience" button.
    ///
    /// Lists Roblox's templates and asks which one; `--baseplate` takes the
    /// stock one without asking, `--template <id>` names one outright.
    ///
    /// Needs no project, no id and no credential. Studio fetches the
    /// template's content and then unbinds the session from it, so the place
    /// exists only in Studio until you save it to Roblox: nothing is created
    /// on Roblox by this flag.
    #[arg(long = "new")]
    pub new_place: bool,

    /// Template place id for `--new`, instead of picking from the list. Any
    /// place Roblox lets Studio open works, including one of your own.
    #[arg(long, requires = "new_place")]
    pub template: Option<u64>,

    /// Take Roblox's baseplate without opening the picker, the way Studio's
    /// own "New Experience" button does. Skips the template listing, so it
    /// needs neither a terminal nor a network call.
    #[arg(long, requires = "new_place", conflicts_with = "template")]
    pub baseplate: bool,

    /// A `.rbxl` / `.rbxlx` to open from disk instead of a published place.
    /// The same as passing the path as the first argument; this spelling is
    /// for paths that do not carry the extension.
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Join the place in the Roblox client instead of opening it in Studio.
    ///
    /// Everything else is unchanged: the same `<env> <place>`, `--place-id`
    /// and `--universe-id` name the place, and no network call is made.
    ///
    /// **It does not choose an account.** The link carries no identity, so the
    /// client joins as whoever is signed into it.
    #[arg(long, conflicts_with = "new_place")]
    pub play: bool,

    /// Place id, instead of naming an env and a place.
    ///
    /// Skips `rbxplace.toml` entirely, so it works in any directory. Wins over
    /// `<env> <place>` when both are given, as the more specific of the two.
    #[arg(long = "place-id", value_name = "ID")]
    pub place_id: Option<u64>,

    /// Universe id: its places are listed from Roblox, and a single-place
    /// universe opens without asking.
    ///
    /// The listing needs no credential, so this also works in any directory.
    #[arg(long = "universe-id", value_name = "ID")]
    pub universe_id: Option<u64>,

    /// Path to `rbxplace.toml`, the file `<env> <place>` is resolved in.
    #[arg(long, default_value = "rbxplace.toml")]
    pub places: PathBuf,
}

/// Which app a place is opened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenIn {
    Studio,
    Player,
}

impl OpenIn {
    fn of(play: bool) -> Self {
        if play {
            Self::Player
        } else {
            Self::Studio
        }
    }

    /// The verb for the progress line: Studio opens a place to edit, the
    /// client joins a running experience.
    fn verb(self) -> &'static str {
        match self {
            Self::Studio => "Opening",
            Self::Player => "Joining",
        }
    }
}

pub async fn run(cli: Cli) -> Result<()> {
    if matches!(cli.env.as_deref(), Some("all")) {
        bail!("`rbx-open` opens one place at a time. Name an env instead of `all`.");
    }

    // A path is the most direct target there is: no id, no config, no network.
    // Recognised by extension rather than by "does this file exist", so an env
    // named `prod` can never be captured by a stray file of the same name.
    if let Some(path) = place_file_target(cli.file.as_deref(), cli.env.as_deref()) {
        if cli.play {
            bail!(
                "`--play` joins a place published on Roblox, and {} is a file on disk: the \
                 client has no id to join. Open it in Studio, or name the place it publishes \
                 to.",
                path.display()
            );
        }
        if cli.new_place
            || cli.place_id.is_some()
            || cli.universe_id.is_some()
            || cli.place.is_some()
        {
            bail!(
                "A place file is opened on its own: `rbx-open {}` names the file to edit, so \
                 there is nothing left for another target to say.",
                path.display()
            );
        }
        let handoff = open_file(&path)?;
        let what = path.display().to_string().cyan().to_string();
        println!("{}", success_line(handoff, "Opening", &what));
        return Ok(());
    }

    // `--new` comes first because it is the only path that needs nothing at
    // all: no file, no id, no network, no credential. Pairing it with a target
    // is a contradiction rather than a preference, so it is refused instead of
    // silently winning.
    if cli.new_place {
        let decided = new_target(
            cli.template,
            cli.baseplate,
            cli.place_id.is_some(),
            cli.universe_id.is_some(),
            cli.env.as_deref(),
            cli.place.as_deref(),
        )?;

        let (place_id, name) = match decided {
            Some(place_id) => (place_id, None),
            None => {
                let templates =
                    list_templates(&rbx_core::api::build_client(), &ApiBase::new(GAMES_HOST))
                        .await?;
                let chosen = pick_template(&templates)?;
                (chosen.place_id, Some(chosen.name.clone()))
            }
        };

        let handoff = open_place(place_id, OpenIn::Studio)?;
        let what = match name {
            Some(name) => format!("a new {} place (template {})", name.cyan(), place_id),
            None => format!("a new place from template {}", place_id.to_string().cyan()),
        };
        println!("{}", success_line(handoff, "Opening", &what));
        // Said out loud because the window looks exactly like an opened place.
        // Studio loads the template's content and then sets the session's place
        // id back to 0, so there is nothing on Roblox for a save to go to yet.
        println!("  Not on Roblox yet: the first save there creates the experience.");
        return Ok(());
    }

    // `--place-id` short-circuits the file entirely. A `roblox-studio:` URI is
    // built out of one number with no network call, so requiring a config file
    // to supply that number would be the only thing tying this to a project.
    let open_in = OpenIn::of(cli.play);

    if let Some(place_id) = cli.place_id {
        let handoff = open_place(place_id, open_in)?;
        let what = format!("place {place_id}");
        println!("{}", success_line(handoff, open_in.verb(), &what));
        return Ok(());
    }

    // The listing needs no credential (see `rbx_core::universe`), so this path
    // works in a bare directory, which is the point: adopting somebody else's
    // universe id is exactly when there is no project to read. None is sent,
    // and none is asked for.
    if let Some(universe_id) = cli.universe_id {
        let places = rbx_core::universe::list_places(
            &rbx_core::api::build_client(),
            &ApiBase::new(DEVELOP_HOST),
            None,
            universe_id,
        )
        .await?;

        let chosen = pick_universe_place(universe_id, &places)?;
        let handoff = open_place(chosen.id, open_in)?;
        let what = format!("{} (place {})", label(chosen).cyan(), chosen.id);
        println!("{}", success_line(handoff, open_in.verb(), &what));
        return Ok(());
    }

    let places = PlacesFile::load(&cli.places)?;

    let env_name = match cli.env {
        Some(name) => name,
        None => pick_env(&places)?,
    };
    let env = places.get(&env_name)?;

    let (place_name, place_id) = resolve_place(&env_name, env, cli.place)?;

    let handoff = open_place(place_id, open_in)?;
    let what = format!(
        "{}/{} (place {})",
        env_name.cyan(),
        place_name.cyan(),
        place_id
    );
    println!("{}", success_line(handoff, open_in.verb(), &what));

    Ok(())
}

/// What to call a place in output. Roblox omits the name on some places, and
/// an entry rendered as an empty string is one the reader cannot pick from.
fn label(place: &UniversePlace) -> String {
    let name = place.name.trim();
    match (name.is_empty(), place.is_root) {
        (true, true) => format!("place {} (start place)", place.id),
        (true, false) => format!("place {}", place.id),
        (false, true) => format!("{name} (start place)"),
        (false, false) => name.to_string(),
    }
}

/// Choose among a universe's places: none is an error, one opens silently,
/// several ask.
fn pick_universe_place(universe_id: u64, places: &[UniversePlace]) -> Result<&UniversePlace> {
    match places {
        // `list_places` seeds the root, so an empty result means the universe
        // reported nothing at all rather than "no extra places".
        [] => bail!(
            "Universe {universe_id} reported no places. Check the id: a place id passed where \
             a universe id belongs is the usual mistake."
        ),
        [only] => Ok(only),
        several => {
            // The id goes on every row. Two places in one universe are allowed
            // to share a display name, and picking blind between two identical
            // rows is how the wrong one gets opened.
            let items: Vec<String> = several
                .iter()
                .map(|p| format!("{}  ({})", label(p), p.id))
                .collect();
            let selection = Select::new()
                .with_prompt(format!("Select a place in universe {universe_id}"))
                .default(0)
                .items(&items)
                .interact()?;
            Ok(&several[selection])
        }
    }
}

fn pick_env(places: &PlacesFile) -> Result<String> {
    let names = places.env_names();
    if names.is_empty() {
        bail!("No environments defined in rbxplace.toml.");
    }
    let selection = Select::new()
        .with_prompt("Select environment")
        .items(&names)
        .interact()?;
    Ok(names[selection].clone())
}

fn resolve_place(
    env_name: &str,
    env: &rbx_core::places::Environment,
    place_choice: Option<String>,
) -> Result<(String, u64)> {
    if let Some(name) = place_choice {
        let id = env.places.get(&name).copied().ok_or_else(|| {
            let mut available: Vec<&str> = env.places.keys().map(|s| s.as_str()).collect();
            available.sort();
            anyhow::anyhow!(
                "Place '{}' not found under [{}.places].\nAvailable: {}",
                name,
                env_name,
                available.join(", ")
            )
        })?;
        return Ok((name, id));
    }

    if env.places.is_empty() {
        bail!(
            "Environment '{}' has no [<env>.places] entries to pick from.",
            env_name
        );
    }

    if env.places.len() == 1 {
        let (k, v) = env
            .places
            .iter()
            .next()
            .expect("len == 1 just checked, entry must exist");
        return Ok((k.clone(), *v));
    }

    let mut names: Vec<&String> = env.places.keys().collect();
    names.sort();
    let display: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let selection = Select::new()
        .with_prompt("Select place")
        .items(&display)
        .interact()?;
    let chosen = names[selection].clone();
    let id = *env
        .places
        .get(&chosen)
        .expect("just selected from this map");
    Ok((chosen, id))
}

/// Choose among Studio's stock templates.
///
/// Roblox returns them newest-first, so the list is shown as `list_templates`
/// ordered it: baseplate first, then the rest as Roblox sees them. The id goes
/// on every row for the same reason it does in the universe picker: it is the
/// only thing guaranteed to tell two rows apart.
fn pick_template(templates: &[StudioTemplate]) -> Result<&StudioTemplate> {
    if templates.is_empty() {
        bail!(
            "Roblox returned no Studio templates. Pass `--baseplate` for the stock one, or \
             `--template <place-id>` for a specific one."
        );
    }

    // A picker with nowhere to draw itself hangs a script forever. `--baseplate`
    // is the escape hatch, so the error names it rather than choosing for them.
    if !rbx_core::output::is_interactive() {
        bail!(
            "`--new` picks a template interactively, and this is not a terminal. Pass \
             `--baseplate`, or `--template <place-id>`."
        );
    }

    let items: Vec<String> = templates
        .iter()
        .map(|t| format!("{}  ({})", t.name, t.place_id))
        .collect();
    let selection = Select::new()
        .with_prompt("Select a template")
        .default(0)
        .items(&items)
        .interact()?;
    Ok(&templates[selection])
}

/// The place file this invocation names, if it names one.
///
/// `--file` is explicit. The positional is taken only when it carries a place
/// file's extension: `rbx-open prod` must stay an environment even in a folder
/// that happens to hold a file called `prod`, so existence on disk is the
/// wrong question to ask.
fn place_file_target(file: Option<&Path>, env: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = file {
        return Some(path.to_path_buf());
    }
    let candidate = env?;
    let lowered = candidate.to_ascii_lowercase();
    (lowered.ends_with(".rbxl") || lowered.ends_with(".rbxlx")).then(|| PathBuf::from(candidate))
}

/// What `--new` opens, and which company it refuses to keep.
///
/// `--new` names no place, so every other way of naming one contradicts it.
/// Picking a winner silently is the failure worth refusing: a flag that
/// parses, is dropped, and leaves the user reading an error about something
/// else entirely.
fn new_target(
    template: Option<u64>,
    baseplate: bool,
    has_place_id: bool,
    has_universe_id: bool,
    env: Option<&str>,
    place: Option<&str>,
) -> Result<Option<u64>> {
    let conflict = if has_place_id {
        Some("--place-id")
    } else if has_universe_id {
        Some("--universe-id")
    } else if env.is_some() || place.is_some() {
        Some("an env/place argument")
    } else {
        None
    };

    if let Some(other) = conflict {
        bail!(
            "`--new` opens a brand-new place, so it cannot be combined with {other}. \
             Drop one: `rbx-open --new` for an empty place, or the target on its own \
             to open a place that already exists."
        );
    }

    if let Some(id) = template {
        return Ok(Some(id));
    }
    if baseplate {
        return Ok(Some(DEFAULT_TEMPLATE_PLACE_ID));
    }
    // Nothing decided it, so the list gets to. `None` means "ask", not "use the
    // baseplate": defaulting quietly here would make `--baseplate` a flag that
    // changes nothing.
    Ok(None)
}

fn open_place(place_id: u64, open_in: OpenIn) -> Result<Handoff> {
    launch(&place_uri(place_id, open_in))
}

/// The URI for one place, in one app.
///
/// The player form is a deep link rather than the `roblox-player:` launch the
/// website sends. That one carries `gameinfo:<ticket>`, a per-account
/// authentication ticket, and the short spelling without it
/// (`roblox-player:1+launchmode:play+placeId:<id>`) was measured on Windows on
/// 2026-09-21: the client starts and stops at its home screen, having nothing
/// to join with. The deep link needs no ticket because the client resolves the
/// place itself, which is also why it cannot pick an account: it joins as
/// whoever is signed in.
fn place_uri(place_id: u64, open_in: OpenIn) -> String {
    match open_in {
        OpenIn::Studio => format!("roblox-studio:1+task:EditPlace+placeId:{place_id}+universeId:0"),
        OpenIn::Player => format!("roblox://experiences/start?placeId={place_id}"),
    }
}

/// Open a `.rbxl` / `.rbxlx` from disk.
///
/// The path is handed to the desktop's opener rather than wrapped in a
/// `roblox-studio:` URI, and deliberately: that URI is parsed by splitting on
/// `+` and `:`, which a Windows path (`C:\\...`) and any filename containing a
/// `+` would both break. The file association reaches the same place: Studio
/// logs `createAndShowIDEDoc with task EditFile` either way.
///
/// Absolute, because the opener does not inherit our working directory.
fn open_file(path: &Path) -> Result<Handoff> {
    if !path.is_file() {
        bail!("No such place file: {}", path.display());
    }
    let absolute = path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", path.display()))?;

    launch(&without_verbatim_prefix(&absolute.to_string_lossy()))
}

/// The path a Windows program reads, from the one `canonicalize()` returns.
///
/// Windows `canonicalize()` returns an extended-length path, which the shell
/// hands to Studio verbatim and Studio does not open. The prefix is an API
/// detail, not part of the path, but it is not the same prefix for every
/// path: a drive path comes back as `\\?\C:\dir\place.rbxl`, and a network
/// path as `\\?\UNC\server\share\dir\place.rbxl`, which has to become
/// `\\server\share\dir\place.rbxl` again rather than lose only the `\\?\`.
///
/// Any other verbatim form (a volume GUID, a device) has no shorter spelling,
/// so it is kept as it is. A path without the prefix, such as every path on
/// macOS and Linux, is returned unchanged.
fn without_verbatim_prefix(path: &str) -> String {
    const VERBATIM: &str = r"\\?\";
    const VERBATIM_UNC: &str = r"\\?\UNC\";

    if path
        .get(..VERBATIM_UNC.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(VERBATIM_UNC))
    {
        return format!(r"\\{}", &path[VERBATIM_UNC.len()..]);
    }
    if let Some(rest) = path.strip_prefix(VERBATIM) {
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return rest.to_string();
        }
    }
    path.to_string()
}

/// How much a launch that did not fail can claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handoff {
    /// The opener took the target and did not report a failure.
    Opened,
    /// The target reached Windows through `rundll32.exe`, which exits 0
    /// whatever happens next: whether anything opened it is unknown.
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    HandedToWindows,
}

/// The line printed once a launch did not fail. `what` names the place.
fn success_line(handoff: Handoff, verb: &str, what: &str) -> String {
    match handoff {
        Handoff::Opened => format!("{} {verb} {what}", "\u{2713}".green()),
        Handoff::HandedToWindows => format!(
            "{} Handed {what} to Windows\n  rundll32.exe cannot tell whether anything opened \
             it: if nothing appears, check that Roblox is installed on the Windows side.",
            "\u{2713}".green()
        ),
    }
}

/// Hand one target (a `roblox-studio:` or `roblox:` URI, or a path) to the
/// desktop.
fn launch(target: &str) -> Result<Handoff> {
    let uri = target;

    #[cfg(target_os = "windows")]
    let handoff = {
        // Hand the URI to `explorer`, which delegates protocol-URI launches to
        // the running desktop shell, and WAIT for it to finish before
        // returning. Both details matter when this tool runs under a launcher
        // that tears down its child process tree on exit (e.g. the rokit
        // trampoline): a fire-and-forget `.spawn()` lets the launcher kill the
        // helper process before the hand-off to the shell completes, so Studio
        // never appears. Blocking on `.status()` until `explorer` has handed the
        // URI off lets the desktop shell start Studio out of our tree.
        //
        // `explorer.exe` is started by its full path, with no shell in between.
        // A shell (`cmd /C explorer ...`) looks in the current directory before
        // anywhere else, so an `explorer.bat` in a cloned project would run in
        // its place.
        let explorer = explorer_exe(std::env::var("SystemRoot").ok().as_deref());
        start_explorer(&explorer, uri)?;
        Handoff::Opened
    };

    #[cfg(target_os = "macos")]
    let handoff = {
        // By its full path, like `explorer.exe` on Windows: `open` is part of
        // macOS and always lives there, so a `PATH` lookup can only find
        // something else. Waited for, and its exit code believed: `open` exits
        // 1 when nothing is registered for the link or the file is missing.
        handed_off(MACOS_OPEN, Command::new(MACOS_OPEN).arg(uri).status())?;
        Handoff::Opened
    };

    #[cfg(target_os = "linux")]
    let handoff = {
        // Studio has no Linux build, so a Linux build of this tool is either
        // running under WSL (where the target has to cross to the Windows host)
        // or has nothing to launch at all. `xdg-open` is tried first anyway: a
        // desktop Linux user with Studio under Wine or Proton has an
        // association for it, and that is theirs to keep.
        let mut tried = Vec::new();
        if !is_wsl() {
            match first_route_that_works(&[Route::new(&["xdg-open"], &[uri])], run_route) {
                Ok(route) => return Ok(route.handoff()),
                Err(outcomes) => tried = outcomes,
            }
        }
        open_on_windows_host(uri, tried)?
    };

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let handoff: Handoff = bail!("Nothing on this platform can open {uri}.");

    Ok(handoff)
}

/// Start `explorer.exe` on one target and wait for it to hand the target off.
///
/// Only a failure to start is an error. Its exit code says nothing:
/// `explorer.exe` exits 1 after a hand-off that worked, so believing it would
/// report every launch as a failure. raw_arg keeps the URI's `+`/`:` intact
/// inside quotes.
#[cfg(target_os = "windows")]
fn start_explorer(explorer: &str, target: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;
    Command::new(explorer)
        .raw_arg(explorer_arg(target))
        .status()
        .with_context(|| format!("Failed to start {explorer}"))?;
    Ok(())
}

/// Whether a launcher whose exit code means something did its job: it
/// started, and it exited 0. Anything else is the error the success line
/// would otherwise have printed over.
#[cfg(any(target_os = "macos", test))]
fn handed_off(program: &str, status: std::io::Result<std::process::ExitStatus>) -> Result<()> {
    let status = status.with_context(|| format!("Failed to start {program}"))?;
    if !status.success() {
        bail!("{program} could not open it ({status})");
    }
    Ok(())
}

/// The macOS opener. Not looked up in `PATH`, for the reason `explorer_exe`
/// gives.
#[cfg(any(target_os = "macos", test))]
const MACOS_OPEN: &str = "/usr/bin/open";

/// Where `explorer.exe` is, from the `SystemRoot` environment variable.
///
/// A valid `SystemRoot` gives the full path. A value that is missing, empty or
/// not absolute gives the bare name instead, never a guessed drive: Windows
/// is not always on `C:`, and a relative root would put the current directory
/// back in front.
///
/// The bare name is safe because no shell resolves it. Rust's
/// `Command::new` on Windows resolves a name that is not absolute itself,
/// searching in order: the child's `PATH` if set explicitly (it is not here),
/// the directory of the running executable, the system directory, the
/// Windows directory, then the parent's `PATH`. The current directory is not
/// on that list (std `Command::new`, "Platform-specific behavior").
/// `explorer.exe` is found in the Windows directory.
#[cfg(any(target_os = "windows", test))]
fn explorer_exe(system_root: Option<&str>) -> String {
    match system_root
        .map(|root| root.trim_end_matches(['\\', '/']))
        .filter(|root| is_absolute_windows_path(root))
    {
        Some(root) => format!(r"{root}\explorer.exe"),
        None => "explorer.exe".to_string(),
    }
}

/// `C:\...` or `\\server\...`, spelled out rather than asked of `Path`, which
/// answers for the platform it was compiled on.
#[cfg(any(target_os = "windows", test))]
fn is_absolute_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    drive || path.starts_with(r"\\")
}

/// `explorer.exe`'s whole command line. It does not split its arguments the
/// way most programs do, so the target goes in one quoted piece. Neither a
/// URI built here nor a Windows path can contain a `"`.
#[cfg(any(target_os = "windows", test))]
fn explorer_arg(target: &str) -> String {
    format!("\"{target}\"")
}

/// Whether this Linux is really a Windows machine wearing a kernel.
///
/// Checked before `xdg-open` rather than after: under WSL the command often
/// exists and "succeeds" without a desktop to hand anything to, so a failure
/// to spawn is not a reliable signal on its own.
#[cfg(target_os = "linux")]
fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::env::var_os("WSLENV").is_some()
    {
        return true;
    }
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| {
            let release = release.to_ascii_lowercase();
            release.contains("microsoft") || release.contains("wsl")
        })
        .unwrap_or(false)
}

/// One way to hand the target over: a program, under each name it may be
/// reached by, its arguments, and what it needs added to its environment.
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Route {
    /// The same program, by name first, then at its usual place on the
    /// Windows drive for a distro that does not append the Windows `PATH` to
    /// its own. A later name is tried only when the one before was not found:
    /// a program that started and failed would only start, and fail, again.
    programs: Vec<&'static str>,
    args: Vec<String>,
    env: Vec<(&'static str, OsString)>,
    /// Whether its exit code says the target was opened. `rundll32.exe`
    /// exits 0 whatever happens to the hand-off, so a success through it
    /// only says the target reached Windows.
    reports_failure: bool,
}

#[cfg(any(target_os = "linux", test))]
impl Route {
    fn new(programs: &[&'static str], args: &[&str]) -> Self {
        Self {
            programs: programs.to_vec(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env: Vec::new(),
            reports_failure: true,
        }
    }

    /// What a success through this route can claim.
    fn handoff(&self) -> Handoff {
        if self.reports_failure {
            Handoff::Opened
        } else {
            Handoff::HandedToWindows
        }
    }
}

/// Run routes in order until one starts and exits 0, and return it.
///
/// Otherwise every attempt is returned, one line each (the program, and its
/// exit status or why it did not start), for the error to show. A program
/// that is missing and one that ran and could not open the target
/// (`xdg-open` with nothing registered for the link, `Start-Process` with no
/// app for it) both send the search on to the next route.
///
/// `run` starts one program of one route. It is a parameter so that the
/// order of attempts is tested without starting anything.
#[cfg(any(target_os = "linux", test))]
fn first_route_that_works(
    routes: &[Route],
    mut run: impl FnMut(&str, &Route) -> std::io::Result<std::process::ExitStatus>,
) -> std::result::Result<&Route, Vec<String>> {
    let mut tried = Vec::new();
    for route in routes {
        for program in &route.programs {
            match run(program, route) {
                Ok(status) if status.success() => return Ok(route),
                Ok(status) => {
                    tried.push(format!("{program}: {status}"));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tried.push(format!("{program}: not found"));
                }
                Err(error) => {
                    tried.push(format!("{program}: could not start ({error})"));
                    break;
                }
            }
        }
    }
    Err(tried)
}

/// Start one program of a route, its output discarded, and wait for it.
#[cfg(target_os = "linux")]
fn run_route(program: &str, route: &Route) -> std::io::Result<std::process::ExitStatus> {
    Command::new(program)
        .args(&route.args)
        .envs(route.env.iter().map(|(key, value)| (*key, value)))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
}

/// The error once every route has failed, with what each one did: "could not
/// reach Studio" alone leaves the reader to guess which of five programs to
/// look at.
#[cfg(any(target_os = "linux", test))]
fn no_route_worked(tried: &[String]) -> anyhow::Error {
    let mut message = String::from("Could not reach Roblox Studio from this Linux. Tried:");
    for line in tried {
        message.push_str("\n  ");
        message.push_str(line);
    }
    message.push_str(
        "\nOutside WSL, `xdg-open` needs an app registered for the link: Studio has no \
         native Linux build. Under WSL, check that interop is enabled and that Studio is \
         installed on the Windows side.",
    );
    anyhow::anyhow!(message)
}

/// The environment variable that carries the target to PowerShell.
#[cfg(any(target_os = "linux", test))]
const TARGET_VAR: &str = "RBX_OPEN_TARGET";

/// What PowerShell runs for a URI. The target is read from the environment,
/// never spliced into this text: inside a quoted PowerShell string, a path is
/// code as soon as it holds a quote PowerShell recognises, and Windows
/// PowerShell recognises more of them than `'` (also U+2018 to U+201B, which
/// a file name can hold). Read from a variable, the value is only ever a
/// value.
#[cfg(any(target_os = "linux", test))]
const START_URI: &str = "Start-Process -FilePath $env:RBX_OPEN_TARGET";

/// What PowerShell runs for a file. `-FilePath` is resolved as a wildcard
/// pattern, so `place[1].rbxl` matches, and opens, a `place1.rbxl` beside it;
/// there is no `-LiteralPath`. Escaping the value makes the pattern match the
/// one literal file. Measured on Windows PowerShell 5.1 with both files
/// present: the raw path resolves to `place1.rbxl`, the escaped one to
/// `place[1].rbxl`.
///
/// URIs are left out of it on purpose. Escaping turns the `?` of
/// `roblox://experiences/start?placeId=...` into `` `? ``, and a URI matches
/// no file, so Start-Process would hand that altered text to Windows as is.
#[cfg(any(target_os = "linux", test))]
const START_FILE: &str =
    "Start-Process -FilePath ([Management.Automation.WildcardPattern]::Escape($env:RBX_OPEN_TARGET))";

/// `WSLENV` with `TARGET_VAR` added, which is what lets a variable set on the
/// Linux side reach a Windows program at all. What is already there is kept
/// byte for byte, without being parsed, so whatever else the user shares
/// still crosses over even when it is not UTF-8.
///
/// It goes without a flag: the value is already in its Windows form (a path
/// went through `wslpath`), and a `roblox-studio:` URI must not be translated.
#[cfg(any(target_os = "linux", test))]
fn wslenv_with_target(existing: Option<&OsStr>) -> OsString {
    let mut wslenv = existing.map(OsStr::to_os_string).unwrap_or_default();
    if !wslenv.is_empty() {
        wslenv.push(":");
    }
    wslenv.push(TARGET_VAR);
    wslenv
}

/// The routes from WSL to the Windows host, in the order they are tried.
///
/// Several routes because which ones exist varies by distro and by how
/// interop was set up.
#[cfg(any(target_os = "linux", test))]
fn windows_host_routes(target: &str, is_file: bool, wslenv: Option<&OsStr>) -> Vec<Route> {
    let start = if is_file { START_FILE } else { START_URI };

    // PowerShell first: Start-Process handles protocol URIs and paths alike.
    let mut powershell = Route::new(
        &[
            "powershell.exe",
            "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
        ],
        &[
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            start,
        ],
    );
    powershell.env = vec![
        (TARGET_VAR, OsString::from(target)),
        ("WSLENV", wslenv_with_target(wslenv)),
    ];

    // Last, because it cannot fail: `rundll32.exe` exits 0 whether or not
    // anything opened the target, so every route that can say "no" goes
    // first. How it handles a path with spaces is untested.
    let mut rundll32 = Route::new(
        &["rundll32.exe", "/mnt/c/Windows/System32/rundll32.exe"],
        &["url.dll,FileProtocolHandler", target],
    );
    rundll32.reports_failure = false;

    // No `cmd.exe /c start` route: cmd reads `&`, `|`, `^` and `%` in the
    // target as its own syntax, which a file name can hold. It would add
    // nothing besides: cmd.exe sits beside rundll32.exe in System32, under
    // the same two names, so when one is out of reach so is the other.
    vec![powershell, Route::new(&["wslview"], &[target]), rundll32]
}

/// Cross from WSL to the Windows host, which is where Studio actually lives,
/// through the first of `windows_host_routes` that works. `tried` is what
/// already failed before this (`xdg-open`), for the error.
///
/// Independent implementation; the gap it closes was
/// pointed out by ROpen 1.3.2, which fixed the same thing in Luau.
#[cfg(target_os = "linux")]
fn open_on_windows_host(target: &str, mut tried: Vec<String>) -> Result<Handoff> {
    // A Linux path means nothing to a Windows program. A `roblox-studio:` URI
    // is not a path and must be handed over untouched. `open_file` passes an
    // absolute path, so a leading `/` is what tells the two apart.
    let is_file = target.starts_with('/');
    let target = if is_file {
        Command::new("wslpath")
            .args(["-w", target])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| {
                let converted = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (!converted.is_empty()).then_some(converted)
            })
            .unwrap_or_else(|| target.to_string())
    } else {
        target.to_string()
    };

    let wslenv = std::env::var_os("WSLENV");
    let routes = windows_host_routes(&target, is_file, wslenv.as_deref());
    match first_route_that_works(&routes, run_route) {
        Ok(route) => Ok(route.handoff()),
        Err(outcomes) => {
            tried.extend(outcomes);
            Err(no_route_worked(&tried))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn place(id: u64, name: &str, is_root: bool) -> UniversePlace {
        UniversePlace {
            id,
            name: name.to_string(),
            is_root,
        }
    }

    /// clap checks a command's definition only when it is built, and panics
    /// on a clash at that moment: on a user's machine, at the first run.
    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The common shapes of an invocation parse into the fields `run` reads.
    #[test]
    fn names_ids_and_files_parse_where_run_expects_them() {
        let cli = Cli::try_parse_from(["rbx-open", "assets", "main"]).unwrap();
        assert_eq!(cli.env.as_deref(), Some("assets"));
        assert_eq!(cli.place.as_deref(), Some("main"));

        let cli = Cli::try_parse_from(["rbx-open", "--place-id", "123", "--play"]).unwrap();
        assert_eq!(cli.place_id, Some(123));
        assert!(cli.play);

        let cli = Cli::try_parse_from(["rbx-open", "x", "--places", "config/places.toml"]).unwrap();
        assert_eq!(cli.places, PathBuf::from("config/places.toml"));
    }

    /// The universe listing answers without a credential, so there is no
    /// cookie to take: not as a flag, and not from `RBX_COOKIE`. Other
    /// arguments are none of this test's business.
    #[test]
    fn no_cookie_is_taken_from_anywhere() {
        assert!(Cli::try_parse_from(["rbx-open", "--universe-id", "1", "--cookie", "x"]).is_err());
        for arg in Cli::command().get_arguments() {
            let id = arg.get_id().as_str().to_ascii_lowercase();
            assert!(!id.contains("cookie"), "{id}");
        }
        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("RBX_COOKIE"), "{help}");
    }

    #[test]
    fn play_and_new_cannot_be_given_together() {
        assert!(Cli::try_parse_from(["rbx-open", "--new", "--play"]).is_err());
    }

    #[test]
    fn a_place_file_is_recognised_by_extension_either_way_it_is_cased() {
        for arg in ["game.rbxl", "Game.RBXLX", "./sub/dir/place.rbxlx"] {
            assert_eq!(
                place_file_target(None, Some(arg)),
                Some(PathBuf::from(arg)),
                "{arg}"
            );
        }
    }

    /// The reason extension beats existence: an env is a name, and a folder
    /// holding a file of that name must not change what the name means.
    #[test]
    fn an_env_name_is_never_mistaken_for_a_file() {
        assert_eq!(place_file_target(None, Some("prod")), None);
        assert_eq!(place_file_target(None, Some("staging.toml")), None);
    }

    #[test]
    fn the_explicit_flag_needs_no_extension() {
        assert_eq!(
            place_file_target(Some(Path::new("weird-name")), None),
            Some(PathBuf::from("weird-name"))
        );
    }

    #[test]
    fn a_drive_path_loses_only_the_verbatim_prefix() {
        assert_eq!(
            without_verbatim_prefix(r"\\?\C:\games\place.rbxl"),
            r"C:\games\place.rbxl"
        );
    }

    /// Stripping `\\?\` alone leaves `UNC\server\share\...`, a relative path
    /// that names nothing.
    #[test]
    fn a_network_path_gets_its_leading_backslashes_back() {
        assert_eq!(
            without_verbatim_prefix(r"\\?\UNC\server\share\dir\place.rbxl"),
            r"\\server\share\dir\place.rbxl"
        );
    }

    #[test]
    fn a_path_without_a_shorter_spelling_is_kept_as_it_is() {
        for path in [
            r"\\server\share\dir\place.rbxl",
            r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\place.rbxl",
            "/home/user/place.rbxl",
        ] {
            assert_eq!(without_verbatim_prefix(path), path);
        }
    }

    /// A bare `explorer`, or one found through a shell, can be an
    /// `explorer.bat` sitting in the current directory.
    #[test]
    fn explorer_is_started_by_its_full_path() {
        assert_eq!(
            explorer_exe(Some(r"D:\Windows")),
            r"D:\Windows\explorer.exe"
        );
        assert_eq!(
            explorer_exe(Some(r"C:\WINDOWS\")),
            r"C:\WINDOWS\explorer.exe"
        );
    }

    #[test]
    fn a_missing_or_relative_system_root_falls_back_to_the_bare_name() {
        // No drive is guessed: Windows is not always on `C:`. std resolves the
        // bare name without ever searching the current directory.
        for root in [
            None,
            Some(""),
            Some("Windows"),
            Some(r".\Windows"),
            Some("C:"),
        ] {
            assert_eq!(explorer_exe(root), "explorer.exe", "{root:?}");
        }
    }

    #[test]
    fn explorer_gets_the_target_in_one_quoted_piece() {
        assert_eq!(
            explorer_arg("roblox-studio:1+task:EditPlace+placeId:123+universeId:0"),
            "\"roblox-studio:1+task:EditPlace+placeId:123+universeId:0\""
        );
        assert_eq!(
            explorer_arg(r"C:\My Games\place.rbxl"),
            "\"C:\\My Games\\place.rbxl\""
        );
    }

    #[test]
    fn the_macos_opener_is_not_looked_up_in_path() {
        assert!(MACOS_OPEN.starts_with('/'), "{MACOS_OPEN}");
    }

    /// Every quote Windows PowerShell closes a single-quoted string on, plus
    /// what would run once one of them had: the target must reach PowerShell
    /// as data, never inside the text it parses.
    #[test]
    fn powershell_never_parses_the_target() {
        let target = "\\\\wsl.localhost\\Ubuntu\\home\\dev\\it's \u{2018}a\u{2019} \
                      \u{201A}b\u{201B}; Remove-Item x; $(calc).rbxl";
        let routes = windows_host_routes(target, true, None);
        let powershells: Vec<&Route> = routes
            .iter()
            .filter(|route| route.programs.iter().all(|p| p.ends_with("powershell.exe")))
            .collect();
        assert_eq!(powershells.len(), 1);

        for route in powershells {
            assert_eq!(route.args.last().map(String::as_str), Some(START_FILE));
            for arg in &route.args {
                for quote in ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'] {
                    assert!(!arg.contains(quote), "{arg:?} holds {quote:?}");
                }
                assert!(!arg.contains("calc"), "{arg:?}");
            }
            assert!(route.env.contains(&(TARGET_VAR, OsString::from(target))));
            assert!(route.env.contains(&("WSLENV", OsString::from(TARGET_VAR))));
        }
    }

    /// A shell would read `&` in `a&b.rbxl` as "then run b.rbxl". Only
    /// PowerShell runs, and it gets the target as a variable.
    #[test]
    fn no_route_hands_the_target_to_a_shell() {
        let target = r"\\wsl.localhost\Ubuntu\home\dev\a&b^c%PATH%.rbxl";
        for route in windows_host_routes(target, true, None) {
            for program in &route.programs {
                let program = program.to_ascii_lowercase();
                assert!(!program.ends_with("cmd.exe"), "{program}");
                if program.ends_with("powershell.exe") {
                    assert!(!route.args.iter().any(|arg| arg.contains(target)));
                }
            }
        }
    }

    fn not_found() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::NotFound, "not found")
    }

    /// Runs `routes` against a stand-in for starting programs, and returns
    /// the outcome and every program "started", in order. Nothing real runs.
    fn simulate(
        routes: &[Route],
        mut answer: impl FnMut(&str) -> std::io::Result<std::process::ExitStatus>,
    ) -> (
        std::result::Result<Vec<&'static str>, Vec<String>>,
        Vec<String>,
    ) {
        let mut started = Vec::new();
        let outcome = first_route_that_works(routes, |program, _| {
            started.push(program.to_string());
            answer(program)
        })
        .map(|route| route.programs.clone());
        (outcome, started)
    }

    /// A PowerShell that ran and exited 1 is the same binary under its full
    /// path: running it again costs a second cold start, and maybe a second
    /// dialog, for the same answer.
    #[test]
    fn a_program_that_ran_and_failed_is_not_started_again_by_its_full_path() {
        let routes = windows_host_routes("roblox-studio:1", false, None);
        let (outcome, started) = simulate(&routes, |program| {
            // rundll32.exe exits 0 whatever happens; a real failure through
            // it does not exist, so it is not asked to fail here.
            if program == "rundll32.exe" {
                Err(not_found())
            } else {
                Ok(exit_status(1))
            }
        });
        assert!(outcome.is_err());
        assert_eq!(
            started,
            [
                "powershell.exe",
                "wslview",
                "rundll32.exe",
                "/mnt/c/Windows/System32/rundll32.exe"
            ]
        );
    }

    /// The same for a program that exists and could not be started: only
    /// "not found" says the full path might do better.
    #[test]
    fn a_program_that_could_not_start_is_not_started_again_by_its_full_path() {
        let routes = windows_host_routes("roblox-studio:1", false, None);
        let (_, started) = simulate(&routes, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied",
            ))
        });
        assert_eq!(started, ["powershell.exe", "wslview", "rundll32.exe"]);
    }

    /// `rundll32.exe` exits 0 whatever happens, so it comes after every route
    /// that can report a failure.
    #[test]
    fn rundll32_is_the_last_resort() {
        let routes = windows_host_routes("roblox-studio:1", false, None);
        let (last, others) = routes.split_last().expect("routes");
        assert_eq!(last.programs[0], "rundll32.exe");
        assert_eq!(last.handoff(), Handoff::HandedToWindows);
        for route in others {
            assert_eq!(route.handoff(), Handoff::Opened, "{:?}", route.programs);
        }
    }

    /// A success through rundll32.exe says the place reached Windows, not
    /// that it opened: a missing Studio exits 0 just the same.
    #[test]
    fn a_hand_off_through_rundll32_does_not_claim_the_place_opened() {
        let routes = windows_host_routes("roblox-studio:1", false, None);
        let (outcome, _) = simulate(&routes, |program| match program {
            "powershell.exe" => Ok(exit_status(1)),
            "rundll32.exe" => Ok(exit_status(0)),
            _ => Err(not_found()),
        });
        assert_eq!(outcome.expect("rundll32.exe took it")[0], "rundll32.exe");
        let route = routes.last().expect("routes");
        let line = success_line(route.handoff(), "Opening", "place 123");
        assert!(line.contains("Handed place 123 to Windows"), "{line}");
        assert!(!line.contains("Opening"), "{line}");
    }

    #[test]
    fn a_route_that_reports_failure_says_the_place_opened() {
        let line = success_line(Handoff::Opened, "Opening", "place 123");
        assert!(line.contains("Opening place 123"), "{line}");
        assert!(!line.contains("Windows"), "{line}");
    }

    #[test]
    fn a_program_not_found_by_name_is_tried_by_its_full_path() {
        let routes = windows_host_routes("roblox-studio:1", false, None);
        let full = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";
        let (outcome, started) = simulate(&routes, |program| {
            if program == full {
                Ok(exit_status(0))
            } else {
                Err(not_found())
            }
        });
        assert!(outcome.is_ok());
        assert_eq!(started, ["powershell.exe", full]);
    }

    /// Each program that was tried, and how it ended, reaches the error.
    #[test]
    fn the_error_says_what_every_route_did() {
        let routes = windows_host_routes("roblox-studio:1", false, None);
        let (outcome, _) = simulate(&routes, |program| match program {
            "powershell.exe" => Ok(exit_status(1)),
            "wslview" => Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied",
            )),
            _ => Err(not_found()),
        });
        let tried = outcome.expect_err("every route failed");
        let message = no_route_worked(&tried).to_string();
        for expected in [
            format!("powershell.exe: {}", exit_status(1)),
            "rundll32.exe: not found".to_string(),
            "/mnt/c/Windows/System32/rundll32.exe: not found".to_string(),
            "wslview: could not start (denied)".to_string(),
        ] {
            assert!(message.contains(&expected), "{expected:?} in {message}");
        }
    }

    #[test]
    fn the_command_reads_the_variable_the_target_is_put_in() {
        for command in [START_URI, START_FILE] {
            assert!(command.contains(&format!("$env:{TARGET_VAR}")), "{command}");
        }
    }

    fn powershell_command(target: &str, is_file: bool) -> String {
        let routes = windows_host_routes(target, is_file, None);
        let powershell = routes
            .iter()
            .find(|route| route.programs[0] == "powershell.exe")
            .expect("a PowerShell route");
        powershell.args.last().expect("a -Command").clone()
    }

    /// `-FilePath` is a wildcard pattern: unescaped, `place[1].rbxl` opens
    /// `place1.rbxl` when both are there.
    #[test]
    fn a_file_is_escaped_for_powershell_wildcards() {
        let command = powershell_command(r"\\wsl.localhost\Ubuntu\home\dev\place[1].rbxl", true);
        assert_eq!(command, START_FILE);
        assert!(
            command.contains("[Management.Automation.WildcardPattern]::Escape("),
            "{command}"
        );
    }

    /// Escaping would turn the `?` in the `--play` link into `` `? ``, which
    /// Start-Process then hands to Windows as is.
    #[test]
    fn a_uri_is_not_escaped() {
        for uri in [
            place_uri(109876543210987, OpenIn::Studio),
            place_uri(109876543210987, OpenIn::Player),
        ] {
            let command = powershell_command(&uri, false);
            assert_eq!(command, START_URI, "{uri}");
            assert!(!command.contains("Escape"), "{uri}");
        }
    }

    /// The variable crosses to Windows only if `WSLENV` names it, and the
    /// user's own entries have to cross as they did before.
    #[test]
    fn wslenv_gains_the_target_and_keeps_the_rest() {
        assert_eq!(wslenv_with_target(None), TARGET_VAR);
        assert_eq!(wslenv_with_target(Some(OsStr::new(""))), TARGET_VAR);
        assert_eq!(
            wslenv_with_target(Some(OsStr::new("WT_SESSION:USERPROFILE/p"))),
            OsString::from(format!("WT_SESSION:USERPROFILE/p:{TARGET_VAR}"))
        );
    }

    /// A value that is not valid UTF-8, the user's own business, is kept as
    /// it is rather than dropped along with every entry beside it.
    #[test]
    fn a_wslenv_that_is_not_utf8_is_kept() {
        let existing = not_utf8();
        assert!(existing.to_str().is_none());
        let mut expected = existing.clone();
        expected.push(format!(":{TARGET_VAR}"));
        assert_eq!(wslenv_with_target(Some(&existing)), expected);
    }

    fn not_utf8() -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            OsStr::from_bytes(b"WT_\xFF:USERPROFILE/p").to_os_string()
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            // A lone surrogate: valid in a Windows string, not in UTF-8.
            let mut wide: Vec<u16> = "WT_".encode_utf16().collect();
            wide.push(0xD800);
            wide.extend(":USERPROFILE/p".encode_utf16());
            OsString::from_wide(&wide)
        }
    }

    fn exit_status(code: u8) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(i32::from(code) << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(u32::from(code))
        }
    }

    #[test]
    fn a_launcher_that_exits_0_handed_the_target_off() {
        assert!(handed_off("/usr/bin/open", Ok(exit_status(0))).is_ok());
    }

    /// `open` and `xdg-open` exit non-zero when nothing can open the target.
    /// Reading only "did it start" printed a success line over that.
    #[test]
    fn a_launcher_that_exits_non_zero_is_an_error() {
        let error = handed_off("/usr/bin/open", Ok(exit_status(1))).expect_err("exit 1");
        assert!(error.to_string().contains("/usr/bin/open"), "{error}");
    }

    #[test]
    fn a_launcher_that_cannot_start_is_an_error() {
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let error = handed_off("xdg-open", Err(missing)).expect_err("never started");
        assert!(error.to_string().contains("xdg-open"), "{error}");
    }

    /// Starts nothing: the path does not exist, so the spawn itself fails,
    /// and that failure must reach the caller instead of a success line.
    #[cfg(windows)]
    #[test]
    fn an_explorer_that_cannot_start_is_an_error() {
        let missing = r"C:\rbx-open-test-does-not-exist\explorer.exe";
        let error = start_explorer(missing, "roblox-studio:1").expect_err("no such file");
        assert!(error.to_string().contains(missing), "{error}");
    }

    #[test]
    fn studio_edits_the_place_and_the_client_joins_it() {
        assert_eq!(
            place_uri(123, OpenIn::Studio),
            "roblox-studio:1+task:EditPlace+placeId:123+universeId:0"
        );
        // The deep link, not `roblox-player:1+launchmode:play+placeId:123`:
        // that one starts the client and stops at its home screen, having no
        // authentication ticket to join with.
        assert_eq!(
            place_uri(123, OpenIn::Player),
            "roblox://experiences/start?placeId=123"
        );
    }

    #[test]
    fn the_progress_line_says_which_app_it_is() {
        assert_eq!(OpenIn::of(false), OpenIn::Studio);
        assert_eq!(OpenIn::of(true), OpenIn::Player);
        assert_eq!(OpenIn::Studio.verb(), "Opening");
        assert_eq!(OpenIn::Player.verb(), "Joining");
    }

    /// Bare `--new` decides nothing on its own: the picker does. Returning the
    /// baseplate here instead would make `--baseplate` a flag that changes
    /// nothing.
    #[test]
    fn new_on_its_own_defers_to_the_picker() {
        let chosen = new_target(None, false, false, false, None, None).expect("nothing to refuse");
        assert_eq!(chosen, None);
    }

    #[test]
    fn baseplate_asks_nobody() {
        let chosen =
            new_target(None, true, false, false, None, None).expect("--baseplate stands alone");
        assert_eq!(chosen, Some(DEFAULT_TEMPLATE_PLACE_ID));
    }

    #[test]
    fn an_explicit_template_wins_over_the_picker() {
        let chosen = new_target(Some(777), false, false, false, None, None)
            .expect("--template stands alone");
        assert_eq!(chosen, Some(777));
    }

    /// Each of these names a place, which is exactly what `--new` says there
    /// isn't one of yet. Accepting the pair and quietly preferring one is what
    /// the error is for.
    #[test]
    fn new_refuses_every_way_of_naming_a_place() {
        for (case, error) in [
            (
                "--place-id",
                new_target(None, false, true, false, None, None),
            ),
            (
                "--universe-id",
                new_target(None, false, false, true, None, None),
            ),
            (
                "env",
                new_target(None, false, false, false, Some("prod"), None),
            ),
            (
                "place",
                new_target(None, false, false, false, None, Some("main")),
            ),
        ] {
            let error = error.expect_err("naming a place contradicts --new");
            let message = error.to_string();
            assert!(message.contains("--new"), "{case}: {message}");
            assert!(message.contains("cannot be combined"), "{case}: {message}");
        }
    }

    /// The whole point of the universe path: one place must not stop to ask.
    /// A prompt here would also hang a non-interactive run.
    #[test]
    fn a_single_place_universe_is_chosen_without_a_prompt() {
        let places = vec![place(111, "Start", true)];
        let chosen = pick_universe_place(7, &places).expect("one place is unambiguous");
        assert_eq!(chosen.id, 111);
    }

    /// `list_places` seeds the root, so an empty vec means the universe
    /// answered with nothing: most often a place id passed as a universe id.
    #[test]
    fn no_places_is_an_error_that_names_the_likely_mistake() {
        let error = pick_universe_place(7, &[]).expect_err("nothing to open");
        assert!(error.to_string().contains("place id"), "got: {error}");
    }

    #[test]
    fn a_nameless_place_is_still_identifiable() {
        assert_eq!(label(&place(111, "", false)), "place 111");
        assert_eq!(label(&place(111, "   ", true)), "place 111 (start place)");
    }

    #[test]
    fn the_start_place_is_marked_so_it_can_be_told_apart() {
        assert_eq!(label(&place(111, "Start", true)), "Start (start place)");
        assert_eq!(label(&place(222, "Lobby", false)), "Lobby");
    }
}
