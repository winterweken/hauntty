# hauntty 👻

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
- **Starship prompt management** — detect, install, browse, and apply official
  [Starship](https://starship.rs) prompt presets (Tokyo Night, Gruvbox Rainbow, Pastel
  Powerline, Pure, and more) with automatic config backups.
- **Import iTerm2 `.itermcolors`** files, converted to Ghostty themes.
- **Fetch more themes** on demand from the upstream
  [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes) catalog.
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

## Usage

```sh
hauntty                       # manage the default Ghostty config
hauntty --config /path/config # operate on a specific config file
hauntty --themes-dir /path    # add a directory to search for themes
```

### Keybindings

| Key | Action |
|-----|--------|
| `Tab` / `1` `2` `3` | switch between Themes, Settings, and Starship |
| `↑ ↓` / `j k` | move selection |
| `/` | filter themes or Starship presets |
| `Enter` | apply theme / edit setting / apply Starship preset |
| `← →` / `h l` | change a setting |
| `i` | import `.itermcolors` (Themes) / install Starship (Starship) |
| `f` | fetch themes from the upstream catalog |
| `s` | save settings changes |
| `?` | help |
| `q` | quit |

> **Note:** Ghostty applies config changes on reload. After hauntty writes your
> config, reload Ghostty with **⌘⇧,** (`cmd+shift+,`) to see the change.

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
scripts/release.sh 0.1.1     # or: patch | minor | major
```

It verifies a clean, green, up-to-date `main`, bumps the version in `Cargo.toml` and the
Homebrew formula, commits and pushes the `vX.Y.Z` tag, waits for the release workflow to
build the platform binaries, writes their real checksums back into `dist/hauntty.rb`, and
syncs the formula into the [Homebrew tap](https://github.com/winterweken/homebrew-tap).
Requires `gh` (authenticated). Override the tap with `TAP_REPO=owner/name`.

## License

MIT © winterweken
