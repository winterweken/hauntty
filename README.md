# hauntty 👻

A fast, keyboard-driven **TUI theme & settings manager for the [Ghostty](https://ghostty.org) terminal**.

Browse every theme with a live truecolor preview, tweak the settings you actually
change, and apply — all without ever hand-editing a config file. hauntty reads and
writes your Ghostty config *surgically*: it only touches the lines it manages and
leaves your comments, formatting, and everything else byte-for-byte intact, with an
automatic timestamped backup on every write.

![hauntty demo](https://raw.githubusercontent.com/winterweken/hauntty/main/demo/hauntty.gif)

## Features

- **Theme browser** with a live preview (ANSI palette, cursor, selection, a syntax
  sample) rendered in true color — no need to apply to see how a theme looks.
- **Fuzzy filter** across all your installed themes.
- **Curated settings** — font, size, opacity, padding, window size, cursor, and more —
  edited through friendly toggles / steppers / selects, never raw text.
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

From the repo (works today, no registry needed):

```sh
cargo install --git https://github.com/winterweken/hauntty --locked
```

Once published to crates.io:

```sh
cargo install hauntty
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

| Key | Action |
|-----|--------|
| `Tab` | switch between Themes and Settings |
| `↑ ↓` / `j k` | move selection |
| `/` | filter themes |
| `Enter` | apply theme / edit setting |
| `← →` / `h l` | change a setting |
| `i` | import a `.itermcolors` file |
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

## Building

```sh
cargo build --release        # all features (import-iterm, online)
cargo build --no-default-features   # core only, no plist/http deps
cargo test                   # unit + round-trip + apply + TUI smoke tests
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
