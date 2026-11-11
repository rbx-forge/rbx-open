# Security policy

`rbx-open` takes no credentials: the listings it reads answer without one. What
it does that matters is hand a link or a file to the system's opener
(`explorer.exe`, `open`, `xdg-open`), built from `rbxplace.toml`, the command
line and what Roblox answers. That makes the interesting reports the ones about
what reaches that opener.

## Reporting a vulnerability

Report it privately, through GitHub: on this repository, open the **Security**
tab and choose **Report a vulnerability**, or go straight to
<https://github.com/rbx-forge/rbx-open/security/advisories/new>.

Do not open a public issue for a vulnerability. What helps: the
`rbx-open --version`, the command you ran, what you expected, what happened,
and why it is a security problem rather than a bug. A proof of concept is
welcome but never required.

**What to expect.** This is a solo-maintained project. An acknowledgement
within a week is the realistic commitment; a fix lands as fast as the severity
warrants. You will be credited in the advisory and the changelog unless you
ask otherwise. There is no bounty program.

## Supported versions

Only the latest release. At 0.x, fixes go out as a new release rather than
being backported.

## What counts

In scope:

- Data from Roblox, or from a `rbxplace.toml` you did not write, that makes
  the opener run a program, open a link other than a `roblox-studio:` or
  `roblox:` one, or open a file you did not name.
- An argument injected into the opener's command line.
- A credential of any kind read, sent or written: this tool should have none.
- Dependency vulnerabilities with a plausible path to any of the above.

Out of scope:

- What Studio or the Roblox app does with a link once it has it. Report that
  to Roblox.
- A file you named being opened by whatever your system associates with it.
- Anything requiring write access to your machine.

## Verifying a download

Each release archive carries a build provenance attestation, a signed
statement of which workflow run and commit produced it:

```sh
gh attestation verify rbx-open-v0.0.0-x86_64-unknown-linux-gnu.zip --repo rbx-forge/rbx-open
```
