#!/usr/bin/env bash
# Snapshot gpui from a Zed release into the `upstream` branch.
#
#   scripts/sync-upstream.sh v1.28.1
#
# `upstream` holds Zed's files untouched: the gpui crates and every Zed workspace crate they
# depend on (see scripts/upstream_crates.py), at their Zed paths, plus Zed's root manifest and
# lockfile under upstream/ for reference. One commit per sync. Afterwards merge it into main:
#
#   git merge upstream
#
# Only the files we changed conflict, because main and upstream share every earlier snapshot.
set -euo pipefail

ZED_REPO="https://github.com/zed-industries/zed.git"
ROOTS=(gpui gpui_platform)

tag="${1:-}"
if [[ -z "$tag" ]]; then
  echo "usage: $0 <zed release tag, e.g. v1.28.1>" >&2
  exit 2
fi

repo="$(git rev-parse --show-toplevel)"
here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'git -C "$repo" worktree remove --force "$work/tree" >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

echo "› fetching Zed $tag manifests"
git clone -q --depth 1 --branch "$tag" --filter=blob:none --no-checkout "$ZED_REPO" "$work/zed"
git -C "$work/zed" sparse-checkout set --no-cone '/Cargo.toml' '/Cargo.lock' '/LICENSE-APACHE' '/crates/**/Cargo.toml'
git -C "$work/zed" -c advice.detachedHead=false checkout -q "$tag"
commit="$(git -C "$work/zed" rev-parse HEAD)"

python3 -I "$here/upstream_crates.py" "$work/zed" "${ROOTS[@]}" > "$work/crates.tsv"
if awk -F'\t' '$3 != "Apache-2.0"' "$work/crates.tsv" | grep -q .; then
  echo "error: a needed crate is not Apache-2.0 — stop and review:" >&2
  awk -F'\t' '$3 != "Apache-2.0"' "$work/crates.tsv" >&2
  exit 1
fi

echo "› fetching $(wc -l < "$work/crates.tsv" | tr -d ' ') crates"
patterns=('/Cargo.toml' '/Cargo.lock' '/LICENSE-APACHE')
while IFS=$'\t' read -r _ dir _; do patterns+=("/$dir/"); done < "$work/crates.tsv"
git -C "$work/zed" sparse-checkout set --no-cone "${patterns[@]}"

if git -C "$repo" show-ref -q --verify refs/heads/upstream; then
  git -C "$repo" worktree add -q "$work/tree" upstream
else
  git -C "$repo" worktree add -q --orphan -b upstream "$work/tree"
fi

tree="$work/tree"
find "$tree" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
mkdir -p "$tree/upstream"
cp "$work/zed/LICENSE-APACHE" "$tree/LICENSE-APACHE"
cp "$work/zed/Cargo.toml" "$tree/upstream/zed-workspace.toml"
cp "$work/zed/Cargo.lock" "$tree/upstream/zed-Cargo.lock"
while IFS=$'\t' read -r _ dir _; do
  # Copy the contents, so a nested crate (refineable/derive_refineable) and its parent
  # land in the same place whichever comes first.
  mkdir -p "$tree/$dir"
  cp -R "$work/zed/$dir/." "$tree/$dir/"
done < "$work/crates.tsv"
{
  echo "# Upstream snapshot"
  echo
  echo "Unmodified files from [zed-industries/zed]($ZED_REPO) (Apache-2.0)."
  echo
  echo "- tag: \`$tag\`"
  echo "- commit: \`$commit\`"
  echo
  echo "| crate | path |"
  echo "|---|---|"
  while IFS=$'\t' read -r name dir _; do echo "| \`$name\` | \`$dir\` |"; done < "$work/crates.tsv"
} > "$tree/upstream/SNAPSHOT.md"

git -C "$tree" add -A
if git -C "$tree" diff --cached --quiet; then
  echo "› upstream already matches Zed $tag"
  exit 0
fi
git -C "$tree" commit -q -m "sync: zed $tag (${commit:0:12})"
echo "› upstream: $(git -C "$repo" log -1 --format='%h %s' upstream)"
echo "  next: git merge upstream"
