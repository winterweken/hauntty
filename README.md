# hauntty 🏚️⚡️👻

A fast, keyboard-driven **TUI theme, settings & prompt manager for the [Ghostty](https://ghostty.org) terminal**.

Browse every theme with a live truecolor preview, tweak the settings you actually
change, manage your [Starship](https://starship.rs) prompt presets, and apply — all
without ever hand-editing a config file. hauntty reads and writes your Ghostty config
*surgically*: it only touches the lines it manages and leaves your comments, formatting,
and everything else byte-for-byte intact, with an automatic timestamped backup on every
write.

![hauntty demo](https://raw.githubusercontent.com/winterweken/hauntty/main/demo/hauntty.gif)

## Features

- **Theme browser** with a live preview (ANSI palette, cursor, selection, a syntax
  sample) rendered in true color — no need to apply to see how a theme looks.
- **Fuzzy filter** across all your installed themes.
- **Curated settings** — font, size, opacity, padding, window size, cursor, shell
  command, shell integration, and more — edited through friendly toggles / steppers /
  selects, never raw text.
- **macOS app icon picker** — set Ghostty's Dock / app-switcher icon to the official
  icon or one of its eight artist-drawn variants, right from the Settings tab.
- **Starship prompt management** — detect, install, browse, and apply official
  [Starship](https://starship.rs) prompt presets (Tokyo Night, Gruvbox Rainbow, Pastel
  Powerline, Pure, and more) with automatic config backups.
- **Import iTerm2 `.itermcolors`** files, converted to Ghostty themes.
- **Fetch more themes** on demand from the upstream
  [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes) catalog.
- **Terminal tool setup** — install lazydocker, Midnight Commander, lazygit,
  tmux, fzf, ripgrep, bat, zoxide, Git, and Docker from the Tools tab, with
  dependency handling and setup notes.
- **Safe by construction** — surgical, comment-preserving edits; atomic writes; a
  timestamped `config.bak.*` before every change; your current inline colors are saved
  as a named theme before switching, so nothing is ever lost.
- Cross-platform (macOS + Linux), single self-contained binary, no runtime deps.

## Install

### Homebrew (macOS/Linux)

```sh
brew install winterweken/tap/hauntty
```

### Cargo

```sh
cargo install hauntty
```

Or build the latest straight from the repo:

```sh
cargo install --git https://github.com/winterweken/hauntty --locked
```

### Prebuilt binary

```sh
curl -fsSL https://raw.githubusercontent.com/winterweken/hauntty/main/install.sh | sh
```

### From source

```sh
git clone https://github.com/winterweken/hauntty
cd hauntty
cargo build --release
# binary at target/release/hauntty
```

> Homebrew, `cargo install hauntty`, and `install.sh` all track the latest **stable**
> release. Release candidates are published separately — see below.

### Release candidates

Development lands on the [`dev` branch](https://github.com/winterweken/hauntty/tree/dev).
When a release is being prepared, `dev` is promoted to a `dev-rc` branch and cut as a
`vX.Y.Z-rc.N` **pre-release** for testing; once it holds up, `dev-rc` merges to `main`
and becomes the stable release. So the path is `dev` → `dev-rc` → `main`.

While a release candidate is open, grab the tarball for your platform from the
[releases page](https://github.com/winterweken/hauntty/releases) — for example, on
Apple Silicon:

```sh
gh release download vX.Y.Z-rc.N --repo winterweken/hauntty --pattern "*aarch64-apple-darwin*"
```

Each asset ships with a `.sha256` checksum. See [What's new](#whats-new) for the
current release candidate and what it contains.

## Usage

```sh
hauntty                       # manage the default Ghostty config
hauntty --config /path/config # operate on a specific config file
hauntty --themes-dir /path    # add a directory to search for themes
```

### Keybindings

| Key | Action |
|-----|--------|
| `Tab` / `1` `2` `3` `4` | switch between Themes, Settings, Starship, and Tools |
| `↑ ↓` / `j k` | move selection |
| `/` | filter themes or Starship presets |
| `Enter` | apply theme / edit setting / apply Starship preset |
| `c` | customize the selected theme's colors (Themes) |
| `p` | open the RGB color picker (theme customizer) |
| `← →` / `h l` | change a setting |
| `i` | import `.itermcolors` (Themes) / install Starship (Starship) |
| `f` | fetch themes (Themes) / presets (Starship) from the upstream catalogs |
| `s` | save settings changes |
| `Enter` / `i` | review installation (Tools); `y` confirms |
| `r` / `b` | refresh tool status / set up Homebrew (Tools) |
| `?` | help |
| `q` | quit |

> **Note:** Ghostty applies config changes on reload. After hauntty writes your
> config, reload Ghostty with **⌘⇧,** (`cmd+shift+,`) to see the change.

### Set up terminal tools

Press `4` for the Tools tab. Choose a tool to see what it does, whether its
executable is installed, the install commands, and any remaining setup steps.
Press `Enter` or `i` to review installation, then `y` to proceed. `Esc` cancels.
Use `PageUp` / `PageDown` to scroll the details and `r` to refresh detection.

Installers run in the normal terminal so you can see progress and answer
package-manager or administrator-password prompts. Press `Enter` afterward to
return to hauntty with your unsaved settings intact. A failed or interrupted
step stops the remaining commands; completed installations are retained.

Homebrew is supported on macOS and Linux; Linux also supports `apt-get`, `dnf`,
and `pacman`. If Homebrew is missing, `b` offers its
[official installer](https://brew.sh/) as a separate confirmed action.
Package managers resolve library dependencies. hauntty also includes Git for
lazygit, a pager for bat, and a Docker runtime for lazydocker when missing.
On Linux without Homebrew, lazydocker and lazygit use their upstream `go install`
routes, with Go and Git installed as needed, and write to `~/.local/bin`.
Add that directory to your shell's `PATH` if needed.

Docker Desktop on macOS still needs its first-run setup; Linux Docker needs a
running service and appropriate user access. An explicit `DOCKER_HOST` or
`DOCKER_CONTEXT` uses the existing Docker CLI instead of installing a local
engine. Compose is optional and is not installed separately by this flow.
See the [lazydocker requirements](https://github.com/jesseduffield/lazydocker#requirements)
and [Docker setup guide](https://docs.docker.com/engine/install/linux-postinstall/).
Shell integrations such as zoxide and fzf have instructions in the tool details;
hauntty does not edit your shell startup files. Installed status checks files,
not service readiness. Package availability and versions depend on your OS and
enabled repositories; installer errors remain visible for troubleshooting.

### Customize a theme

Select a theme on the Themes tab and press `c`. Use `↑ ↓` to choose the
background, foreground, cursor, selection, or one of the 16 ANSI palette colors.
Press `Enter` to edit a color as `#RRGGBB` or `#RGB` (`Ctrl-U` clears the field).
Submitting a color updates the preview; `r` restores that color to its starting
value. Non-hex color values are preserved, but the preview uses fallbacks for them.

Press `p` to open the terminal color picker. Use `↑ ↓` to select Brightness,
Red, Green, or Blue, then `← →` (or `- +`) to turn that control Down or Up.
Choose `1` Fine, `2` Medium, or `3` Coarse steps (1, 8, or 16 out of 255);
`Tab` also cycles steps. Color ramps and Down/Current/Up swatches accompany the
live theme preview. `Enter` keeps the color, `Esc` cancels the picker changes,
and `r` resets the picker to its opening color. Brightness adjusts all three RGB
channels equally, clamping at 0 and 255. For a default or unmodeled color, the
picker starts at neutral gray; accepting without a change preserves the original
value. Hex entry remains available with `Enter` from the color list.

Press `s` and enter a new name to save a custom copy. The copy is selected in
the theme list; press `Enter` to apply it through the usual confirmation.
The source theme stays intact, and saving a copy does not change your Ghostty
config. When customizing the active theme, its inline color overrides are
included. `Esc` leaves the editor, with a second press required to discard edits.
Font, opacity, padding, and other preferences remain on the Settings tab.

### Import a theme

Press `i` on the Themes tab, then drag a `.itermcolors` file into the terminal
or paste/type its path. Press `Enter` to import. Paths with spaces, single or
double quotes, shell-escaped characters, and `~/` are accepted. Import one file
at a time; an error leaves the path open for correction (`Ctrl-U` clears it).
After import, the list refreshes, clears any search filter, and selects the new
theme for preview. Press `Enter` again to apply it.

If you prefer to browse, press `Tab` in the import field. Use `↑ ↓` to select,
`Enter` to open a folder or choose a file, and `←` to go to the parent folder.
Choosing a file fills the path; press `Enter` again to import it. `Tab` or `Esc`
returns from the browser to the path field.

## What's new

### Release candidate — v1.1.0-rc.1 (from `dev`)

- **Customize a theme's colors in place** — press `c` on the Themes tab to edit the
  background, foreground, cursor, selection, and all 16 ANSI colors against the live
  preview, then `s` to save the result as a new named theme. The source theme and your
  config are left untouched.
- **Interactive RGB color picker** — press `p` while customizing to dial a color in
  with Brightness / Red / Green / Blue controls, three step sizes, and
  Down / Current / Up swatches, instead of typing hex by hand.
- **Tools tab** — detect and install lazydocker, Midnight Commander, lazygit, tmux,
  fzf, ripgrep, bat, zoxide, Git, and Docker, with dependency handling, per-tool setup
  notes, and a confirmation step before any installer runs.
- **Browse for the file to import** — the `.itermcolors` import prompt now includes a
  file browser (`Tab`), alongside drag-and-drop and typed paths.
- **Starship presets now match your installed Starship.** Presets were listed from the
  starship repo's `main` branch, so they could reference modules newer than any
  released binary — `catppuccin-powerline` gaining `[jj_bookmark]` produced
  `Error in 'StarshipRoot' at 'jj_bookmark': Unknown key` on every prompt. hauntty now
  pins the catalog to the release tag matching your `starship --version`, falling back
  to `main` only when Starship isn't installed or that tag doesn't exist upstream.

### v1.0.0

- **Theme backups can no longer lose colors.** The backup written before a theme apply
  now captures the *effective* look: repeated keys follow Ghostty's last-one-wins rule,
  values hauntty can't model as RGB (named X11 colors, `cell-foreground` /
  `cell-background`, palette indices 16–255) are preserved verbatim — inline or
  inherited from the base theme — and applying over a conditional
  `theme = dark:…,light:…` line refuses rather than writing a lossy backup.
- **Settings are safer.** A repeated key (e.g. a `font-family` fallback stack) shows as
  "(multiple entries)" and can no longer be wiped from the editor; text settings
  prefill their real current value.
- **macOS app icon setting** — pick between Ghostty's official icon and its eight
  artist-drawn variants; `block_hollow` also joins the cursor styles.
- **Cursor style, explained.** Ghostty's shell integration forces a bar cursor at the
  prompt; when you change the cursor style, hauntty now points you at
  `shell-integration-features = no-cursor` so the change actually sticks.
- **Starship apply hardening** — the config's file permissions (e.g. `0600`) are
  preserved or the apply aborts cleanly, plus a broad batch of review fixes across the
  app (input handling, path guards, MSRV 1.88).

> **Why 1.0.0 and not 0.1.4.** This release changes the library target's API
> (`apply::apply_theme` takes a base theme, `theme::Theme` gained `raw_extras`,
> `starship::StarshipPreset` holds `Cow<'static, str>`). In a `0.x` crate Cargo treats
> the *minor* as the compatibility boundary, so `^0.1.3` would have picked up a `0.1.4`
> that no longer compiles for it. Leaving `0.x` puts hauntty on ordinary semver —
> `1.0.1` for fixes, `1.1.0` for additions, `2.0.0` for breaks — and release candidates
> continue as `vX.Y.Z-rc.N` pre-releases.
>
> The published library target (`hauntty` as a crate dependency) exists to serve the
> binary and the integration tests; it carries **no API stability guarantee** and may
> change in any release. Depend on the `hauntty` binary, not the lib.

### Earlier releases

- **v0.1.3** — fetch, preview, and apply Starship presets from the full
  [official catalog](https://starship.rs/presets/), beyond the bundled eight.
- **v0.1.2** — published to [crates.io](https://crates.io/crates/hauntty); hardening:
  panic hooks, path sanitization, config file locking, more robust color parsing.
- **v0.1.1** — Starship prompt management (status, install, preset browser) and shell
  settings; atomic writes resolve symlinks so dotfile-managed configs stay symlinked.
- **v0.1.0** — first release: theme browser with live preview, curated settings,
  `.itermcolors` import, and theme fetching.

## How applying a theme works

If your config sets colors inline (a `palette = …` block with no `theme =` line),
hauntty first saves those colors as a named theme in `~/.config/ghostty/themes/`, then
replaces the inline block with a single `theme = <Name>` line. Your old look isn't lost
— it shows up in the theme list as a `user` theme you can switch back to any time.

## Starship prompt management

The **Starship** tab lets you manage your [Starship](https://starship.rs) cross-shell
prompt without leaving hauntty:

- **Status detection** — shows whether `starship` is installed, its version, and the
  path to `~/.config/starship.toml`.
- **One-key install** — press `i` to install Starship via Homebrew (or the official
  install script as a fallback).
- **Preset browser** — browse and preview 8 curated official presets (Nerd Font
  Symbols, No Nerd Fonts, Tokyo Night, Pastel Powerline, Gruvbox Rainbow, Pure,
  Bracketed Segments, Plain Text ASCII) with a live TOML preview.
- **Fetch the full catalog** — press `f` to pull every official preset. The listing is
  pinned to the release tag matching your installed `starship --version`, so a fetched
  preset never references a module your binary doesn't have yet. Without Starship
  installed (or if the tag is missing upstream) it falls back to the repo's `main`.
- **Safe apply** — writes `~/.config/starship.toml` with a timestamped backup
  (`starship.toml.bak.<timestamp>`) created automatically.
- **Links** — docs at [starship.rs](https://starship.rs) and the full presets catalog
  at [starship.rs/presets](https://starship.rs/presets/).

## Building

```sh
cargo build --release        # all features (import-iterm, online)
cargo build --no-default-features   # core only, no plist/http deps
cargo test                   # unit + round-trip + apply + starship + TUI smoke tests
```

The config round-trip is covered by a test that parses and re-renders **every** Ghostty
theme file installed on your machine and asserts byte-for-byte equality.

## Releasing

Cutting a release is one command ([`scripts/release.sh`](scripts/release.sh)):

```sh
scripts/release.sh 1.1.0     # or: patch | minor | major | final
```

`final` drops a pre-release suffix (`1.1.0-rc.3` → `1.1.0`) — the normal way to ship an
RC that has held up. Bumping `patch` from a pre-release is refused, since `1.1.0-rc.3`
sorts *below* `1.1.0` and would skip the release the RC was previewing.

It verifies a clean, green, up-to-date `main`, bumps the version in `Cargo.toml` and the
Homebrew formula, commits and pushes the `vX.Y.Z` tag, waits for the release workflow to
build the platform binaries, writes their real checksums back into `dist/hauntty.rb`, and
syncs the formula into the [Homebrew tap](https://github.com/winterweken/homebrew-tap).
Requires `gh` (authenticated). Override the tap with `TAP_REPO=owner/name`.

## Credits

- [Ghostty](https://ghostty.org) by [Mitchell Hashimoto](https://github.com/mitchellh)
  and the [Ghostty contributors](https://github.com/ghostty-org/ghostty) — the terminal
  itself, its bundled theme collection, and the artist-drawn app-icon variants that the
  icon setting selects between.
- The color schemes fetched with `f` come from
  [mbadolato/iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes) —
  the catalog Ghostty's own bundled themes are generated from. Each scheme belongs to
  its original author, collected and converted there.
- [Starship](https://starship.rs) ([starship/starship](https://github.com/starship/starship))
  and its official [preset gallery](https://starship.rs/presets/), which the Starship
  tab installs and fetches from.

## License

MIT © winterweken
