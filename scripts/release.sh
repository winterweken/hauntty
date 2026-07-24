#!/usr/bin/env bash
#
# release.sh — cut a hauntty release end to end.
#
# Usage:
#   scripts/release.sh <version>     e.g. scripts/release.sh 0.1.1
#   scripts/release.sh patch|minor|major   (bump from the current version)
#
# What it does:
#   1. Verifies a clean, green, up-to-date checkout.
#   2. Bumps the version in Cargo.toml (+ Cargo.lock) and dist/hauntty.rb.
#   3. Commits, tags vX.Y.Z, and pushes — triggering the release workflow.
#   4. Waits for the workflow to build and publish the platform binaries.
#   5. Pulls the real checksums, writes them into the formula, and commits.
#   6. Syncs the formula into the Homebrew tap repo.
#
# Requirements: git, gh (authenticated), cargo, perl.
# Env overrides:
#   REMOTE     git remote name for this repo   (default: origin)
#   TAP_REPO   owner/name of the Homebrew tap  (default: winterweken/homebrew-tap)
set -euo pipefail

REMOTE="${REMOTE:-origin}"
TAP_REPO="${TAP_REPO:-winterweken/homebrew-tap}"

die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
step() { printf '\n\033[35m==>\033[0m \033[1m%s\033[0m\n' "$*"; }

# --- locate repo root -------------------------------------------------------
cd "$(git rev-parse --show-toplevel 2>/dev/null)" || die "not inside a git repo"
[ -f Cargo.toml ] || die "Cargo.toml not found at repo root"

# --- preconditions ----------------------------------------------------------
command -v gh   >/dev/null || die "gh (GitHub CLI) is required"
command -v cargo>/dev/null || die "cargo is required"
command -v perl >/dev/null || die "perl is required"
gh auth status >/dev/null 2>&1 || die "gh is not authenticated (run: gh auth login)"

[ "$(git rev-parse --abbrev-ref HEAD)" = "main" ] || die "release from the 'main' branch"
git diff --quiet && git diff --cached --quiet || die "working tree is dirty; commit or stash first"
git fetch "$REMOTE" --quiet
[ "$(git rev-parse HEAD)" = "$(git rev-parse "$REMOTE/main")" ] || die "local main is not in sync with $REMOTE/main"

# --- resolve the target version --------------------------------------------
current="$(perl -ne 'print $1 if /^version = "(\d+\.\d+\.\d+)"/' Cargo.toml)"
[ -n "$current" ] || die "could not read current version from Cargo.toml"

arg="${1:-}"
[ -n "$arg" ] || die "usage: scripts/release.sh <version|patch|minor|major>"

case "$arg" in
  patch|minor|major)
    IFS=. read -r MA MI PA <<<"$current"
    case "$arg" in
      patch) PA=$((PA + 1)) ;;
      minor) MI=$((MI + 1)); PA=0 ;;
      major) MA=$((MA + 1)); MI=0; PA=0 ;;
    esac
    VERSION="$MA.$MI.$PA"
    ;;
  *)
    [[ "$arg" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must be X.Y.Z (got '$arg')"
    VERSION="$arg"
    ;;
esac

TAG="v$VERSION"
git rev-parse "$TAG" >/dev/null 2>&1 && die "tag $TAG already exists"
printf 'Releasing \033[1m%s\033[0m -> \033[1m%s\033[0m\n' "$current" "$VERSION"

# --- bump versions ----------------------------------------------------------
step "Bumping version to $VERSION"
perl -pi -e "s/^version = \"\d+\.\d+\.\d+\"/version = \"$VERSION\"/" Cargo.toml
perl -pi -e "s/^(\s*)version \"\d+\.\d+\.\d+\"/\${1}version \"$VERSION\"/" dist/hauntty.rb

# --- verify green -----------------------------------------------------------
step "Verifying (fmt, clippy, test) — also refreshes Cargo.lock"
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# --- commit + tag + push ----------------------------------------------------
step "Committing and tagging $TAG"
git add Cargo.toml Cargo.lock dist/hauntty.rb
git commit -q -m "Release $TAG"
git tag -a "$TAG" -m "hauntty $TAG"
git push -q "$REMOTE" main
git push -q "$REMOTE" "$TAG"

# --- wait for the release workflow -----------------------------------------
step "Waiting for the release workflow to build $TAG"
run_id=""
for _ in $(seq 1 30); do
  run_id="$(gh run list --workflow=release.yml --json databaseId,headBranch \
    --jq "[.[] | select(.headBranch==\"$TAG\")][0].databaseId" 2>/dev/null || true)"
  [ -n "$run_id" ] && [ "$run_id" != "null" ] && break
  sleep 4
done
[ -n "$run_id" ] && [ "$run_id" != "null" ] || die "could not find a release run for $TAG"
gh run watch "$run_id" --exit-status --interval 20 >/dev/null \
  || die "release workflow failed; see: gh run view $run_id"

# --- update formula checksums ----------------------------------------------
step "Writing real checksums into dist/hauntty.rb"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
gh release download "$TAG" --pattern "*.sha256" --dir "$tmp" >/dev/null

for f in "$tmp"/*.sha256; do
  sha="$(awk '{print $1}' "$f")"
  fname="$(awk '{print $2}' "$f")"
  target="$(printf '%s' "$fname" | perl -pe "s/^hauntty-v[0-9.]+-(.*)\.tar\.gz\$/\$1/")"
  # Replace the sha256 line that follows the url ending in this target.
  perl -0777 -pi -e \
    "s/(-\\Q$target\\E\\.tar\\.gz\"\\s*\\n\\s*sha256 \")[0-9a-fA-F]{64}/\${1}$sha/g" \
    dist/hauntty.rb
done

grep -q "REPLACE_WITH" dist/hauntty.rb && die "some checksums were not filled in"
ruby -c dist/hauntty.rb >/dev/null 2>&1 || die "dist/hauntty.rb has invalid Ruby syntax"

step "Committing formula update"
git add dist/hauntty.rb
git commit -q -m "Update Homebrew formula for $TAG"
git push -q "$REMOTE" main

# --- sync the tap -----------------------------------------------------------
step "Syncing $TAP_REPO"
taproot="$tmp/tap"
if gh repo clone "$TAP_REPO" "$taproot" -- --quiet 2>/dev/null; then
  mkdir -p "$taproot/Formula"
  cp dist/hauntty.rb "$taproot/Formula/hauntty.rb"
  if git -C "$taproot" diff --quiet; then
    echo "tap already up to date"
  else
    git -C "$taproot" add Formula/hauntty.rb
    git -C "$taproot" commit -q -m "hauntty $TAG"
    git -C "$taproot" push -q
    echo "tap updated"
  fi
else
  printf '\033[33mwarning:\033[0m could not clone %s — update its Formula/hauntty.rb manually.\n' "$TAP_REPO"
fi

# --- done -------------------------------------------------------------------
step "Released $TAG 🎉"
cat <<EOF
  Release: https://github.com/${GITHUB_REPOSITORY:-winterweken/hauntty}/releases/tag/$TAG
  Install: brew install ${TAP_REPO%/*}/tap/hauntty
EOF
