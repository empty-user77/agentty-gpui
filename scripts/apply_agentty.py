"""Turn the Zed snapshot into the agentty-gpui workspace.

    python3 -I scripts/apply_agentty.py

Writes the root Cargo.toml and edits every crate manifest so the crates publish to crates.io
under our names. Idempotent: run it again after `git merge upstream`. If a crate manifest
conflicts in that merge, take Zed's side (`git checkout --theirs <file>`) and rerun this.

What it does, and why:
- package `gpui` -> `agentty-gpui`, `gpui_macos` -> `agentty-gpui-macos`, `collections` ->
  `agentty-gpui-collections`, ... The `[lib] name` stays Zed's, so code keeps `use gpui::...`.
- every crate takes its version, `publish` and `repository` from the workspace.
- leaves out the iOS and web platforms and the dev-only crates we don't ship (DROPPED).
- git dependencies become the crates.io releases in CRATES_IO (crates.io takes nothing else).
"""

import pathlib
import re
import sys
import tomllib

VERSION = "0.1.0"
REPOSITORY = "https://github.com/empty-user77/agentty-gpui"

# Zed crates in the snapshot that are not part of this workspace.
DROPPED = {"gpui_ios", "gpui_web", "reqwest_client", "util_macros"}
# Examples that need a dropped crate.
DROPPED_EXAMPLES = {"gpui": {"image", "image_gallery"}}
# Zed takes these from Git; we take the crates.io release instead. See README "Dependencies".
CRATES_IO = {
    "async-task": {"version": "4.7.1"},
    "font-kit": {"version": "0.14.1-zed"},
    "scap": {"version": "0.0.8-zed"},
    "oo7": {"version": "0.6"},
    "wgpu": {"version": "30.0.2"},
    "wasm_thread": {"version": "0.3"},
    # Declared in a crate manifest, not the workspace.
    "naga": {"version": "30.0.2"},
    "xim": {},  # already names zed-xim 0.4.0-zed on crates.io
}

# gpui_apple's build script reads these gpui sources (to generate the Metal shader header).
# A published crate can't reach ../gpui, so it carries a copy, refreshed here.
APPLE_GPUI_SOURCES = "crates/gpui_apple/gpui-src"

MANIFEST_NOTE = "# Modified for Agentty by scripts/apply_agentty.py (package name, version, dependencies)."

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZED_WORKSPACE = ROOT / "zed" / "zed-workspace.toml"


def package_name(crate: str) -> str:
    if crate == "gpui":
        return "agentty-gpui"
    return "agentty-gpui-" + crate.removeprefix("gpui_").replace("_", "-")


def snapshot_crates() -> dict[str, str]:
    """Crate name -> path, from zed/SNAPSHOT.md."""
    crates = {}
    for line in (ROOT / "zed" / "SNAPSHOT.md").read_text().splitlines():
        cells = [c.strip(" `") for c in line.split("|")]
        if len(cells) == 4 and cells[1] and cells[2].startswith("crates/"):
            crates[cells[1]] = cells[2]
    return crates


# --- TOML writing (stdlib has a reader only) -------------------------------------------------


def toml_value(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{toml_key(k)} = {toml_value(v)}" for k, v in value.items()) + " }"
    raise TypeError(f"can't write {value!r}")


def toml_key(key: str) -> str:
    return key if re.fullmatch(r"[A-Za-z0-9_-]+", key) else toml_value(key)


def toml_table(name: str, table: dict) -> list[str]:
    lines = [f"[{name}]"]
    for key, value in table.items():
        lines.append(f"{toml_key(key)} = {toml_value(value)}")
    return lines + [""]


# --- crate manifests ----------------------------------------------------------------------------


def sections(lines: list[str]) -> list[tuple[str, int, int]]:
    """(header, first line, end line) for each table, the preamble as header ''."""
    starts = [(0, "")]
    for i, line in enumerate(lines):
        m = re.match(r"^\s*(\[\[?[^\]]+\]\]?)\s*(#.*)?$", line)
        if m:
            starts.append((i, m.group(1)))
    out = []
    for n, (i, header) in enumerate(starts):
        end = starts[n + 1][0] if n + 1 < len(starts) else len(lines)
        out.append((header, i, end))
    return out


def set_package_key(lines: list[str], key: str, value_line: str) -> list[str]:
    """Replace `key = ...` / `key.workspace = ...` in [package], or add it after `name`."""
    for header, start, end in sections(lines):
        if header != "[package]":
            continue
        pattern = re.compile(rf"^{re.escape(key)}(\.workspace)?\s*=")
        for i in range(start + 1, end):
            if pattern.match(lines[i]):
                lines[i] = value_line
                return lines
        name_at = next(i for i in range(start + 1, end) if re.match(r"^name\s*=", lines[i]))
        lines.insert(name_at + 1, value_line)
        return lines
    raise ValueError("no [package] table")


def ensure_lib_name(lines: list[str], lib: str) -> list[str]:
    for header, start, end in sections(lines):
        if header == "[lib]":
            for i in range(start + 1, end):
                if re.match(r"^name\s*=", lines[i]):
                    lines[i] = f'name = "{lib}"'
                    return lines
            lines.insert(start + 1, f'name = "{lib}"')
            return lines
    # No [lib] table: add one right after [package].
    _, start, end = next(s for s in sections(lines) if s[0] == "[package]")
    while end > start and not lines[end - 1].strip():
        end -= 1
    return lines[:end] + ["", "[lib]", f'name = "{lib}"'] + lines[end:]


def drop_dependencies(lines: list[str]) -> list[str]:
    """Drop one-line dependencies on DROPPED crates and features that point at them."""
    dropped = "|".join(re.escape(c) for c in sorted(DROPPED))
    dep_line = re.compile(rf"^\s*({dropped})(\.workspace\s*=|\s*=)")
    feature_ref = re.compile(rf'"(dep:)?({dropped})(\?)?(/[^"]*)?",?\s*')
    out = []
    for header, start, end in sections(lines):
        block = lines[start:end]
        if "dependencies" in header:
            block = [line for line in block if not dep_line.match(line)]
        elif header == "[features]":
            block = [feature_ref.sub("", line) for line in block]
            block = [re.sub(r",\s*\]", "]", line) for line in block]
        out += block
    return out


def drop_examples(lines: list[str], names: set[str]) -> list[str]:
    out = []
    for header, start, end in sections(lines):
        block = lines[start:end]
        if header == "[[example]]":
            name = next((re.match(r'^name\s*=\s*"([^"]+)"', l) for l in block if l.startswith("name")), None)
            if name and name.group(1) in names:
                continue
        out += block
    return out


def drop_git_sources(lines: list[str]) -> list[str]:
    """`dep = { git = ..., rev = ..., ... }` in a crate manifest -> its crates.io release."""
    out = []
    for line in lines:
        m = re.match(r'^(\s*)([A-Za-z0-9_-]+)(\s*=\s*\{.*)\bgit\s*=\s*"[^"]*",?\s*', line)
        if m:
            dep = m.group(2)
            if dep not in CRATES_IO:
                sys.exit(f"error: {dep} comes from Git in a crate manifest; add it to CRATES_IO")
            line = re.sub(r'\bgit\s*=\s*"[^"]*",?\s*', "", line)
            line = re.sub(r'\b(rev|branch|tag)\s*=\s*"[^"]*",?\s*', "", line)
            if "version" in CRATES_IO[dep] and "version" not in line:
                line = line.replace("{ ", f'{{ version = "{CRATES_IO[dep]["version"]}", ', 1)
        out.append(line)
    return out


def path_only_dev_dependencies(lines: list[str], crate_dir: str, ours: dict[str, str], zed_deps: dict) -> list[str]:
    """Dev-dependencies on our own crates by path, without a version.

    `cargo publish` drops such dev-dependencies, which breaks the cycles (gpui_macros tests
    use gpui, gpui uses gpui_macros) that would otherwise stop the workspace publishing.
    """
    out = []
    for header, start, end in sections(lines):
        block = lines[start:end]
        if header.endswith("dev-dependencies]"):
            new_block = []
            for line in block:
                m = re.match(r"^([A-Za-z0-9_-]+)(?:\.workspace\s*=\s*true|\s*=\s*\{(.*)\})\s*$", line)
                dep = m and m.group(1)
                if dep in ours:
                    rest = m.group(2) or ""
                    if "workspace" not in rest and m.group(2) is not None:
                        new_block.append(line)  # already path-only
                        continue
                    rest = re.sub(r"\bworkspace\s*=\s*true,?\s*", "", rest).strip().rstrip(",")
                    rel = pathlib.Path(ours[dep]).relative_to(crate_dir, walk_up=True)
                    spec = f'path = "{rel}", package = "{package_name(dep)}"'
                    zed_spec = zed_deps.get(dep, {})
                    if "default-features" in zed_spec and "default-features" not in rest:
                        spec += f", default-features = {toml_value(zed_spec['default-features'])}"
                    if rest:
                        spec += f", {rest}"
                    line = f"{dep} = {{ {spec} }}"
                elif re.match(r"^([A-Za-z0-9_-]+)\s*=\s*\{[^}]*$", line) and line.split()[0] in ours:
                    sys.exit(f"error: multi-line dev-dependency on {line.split()[0]} in {crate_dir}")
                new_block.append(line)
            block = new_block
        out += block
    return out


def copy_apple_gpui_sources() -> None:
    build = (ROOT / "crates/gpui_apple/build.rs").read_text()
    files = sorted(set(re.findall(r'gpui_dir\.join\("(src/[^"]+)"\)', build)))
    if not files:
        sys.exit("error: gpui_apple/build.rs no longer reads gpui sources; check APPLE_GPUI_SOURCES")
    target = ROOT / APPLE_GPUI_SOURCES
    for old in target.rglob("*.rs") if target.exists() else []:
        if str(old.relative_to(target)) not in files:
            old.unlink()
    for rel in files:
        dest = target / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes((ROOT / "crates/gpui" / rel).read_bytes())


def edit_crate(crate: str, path: str, ours: dict[str, str], zed_deps: dict) -> None:
    manifest = ROOT / path / "Cargo.toml"
    lines = manifest.read_text().splitlines()
    package = tomllib.loads(manifest.read_text())["package"]
    if not lines or lines[0] != MANIFEST_NOTE:
        lines.insert(0, MANIFEST_NOTE)

    lines = set_package_key(lines, "name", f'name = "{package_name(crate)}"')
    lines = set_package_key(lines, "version", "version.workspace = true")
    lines = set_package_key(lines, "publish", "publish.workspace = true")
    lines = set_package_key(lines, "repository", "repository.workspace = true")
    if "description" not in package:
        lines = set_package_key(
            lines, "description", f'description = "Zed\'s {crate} crate, as used by agentty-gpui"'
        )
    lines = ensure_lib_name(lines, crate)
    lines = drop_dependencies(lines)
    lines = drop_git_sources(lines)
    lines = path_only_dev_dependencies(lines, path, ours, zed_deps)
    if crate in DROPPED_EXAMPLES:
        lines = drop_examples(lines, DROPPED_EXAMPLES[crate])
        # Otherwise cargo finds the dropped examples' files again on its own.
        lines = set_package_key(lines, "autoexamples", "autoexamples = false")

    text = "\n".join(lines).rstrip("\n") + "\n"
    tomllib.loads(text)  # still valid TOML
    manifest.write_text(text)


# --- root manifest --------------------------------------------------------------------------


def used_dependencies(paths: list[str]) -> set[str]:
    used = set()
    for path in paths:
        manifest = tomllib.loads((ROOT / path / "Cargo.toml").read_text())
        tables = [manifest]
        tables += list(manifest.get("target", {}).values())
        for t in tables:
            for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
                for dep, spec in t.get(kind, {}).items():
                    if isinstance(spec, dict) and spec.get("workspace"):
                        used.add(dep)
    return used


def workspace_dependency(dep: str, spec, ours: set[str]) -> dict | str:
    if not isinstance(spec, dict):
        return spec
    spec = dict(spec)
    if "path" in spec:
        if dep not in ours:
            sys.exit(f"error: {dep} is a Zed crate outside this workspace")
        spec["package"] = package_name(dep)
        spec["version"] = VERSION
        return spec
    if "git" in spec:
        if dep not in CRATES_IO:
            sys.exit(f"error: {dep} comes from Git in Zed; add its crates.io version to CRATES_IO")
        for key in ("git", "rev", "branch", "tag"):
            spec.pop(key, None)
        spec.update(CRATES_IO[dep])
    return spec


def write_root(ours: dict[str, str]) -> None:
    zed = tomllib.loads(ZED_WORKSPACE.read_text())
    workspace = zed["workspace"]
    members = sorted(ours.values())
    used = used_dependencies(members)
    missing = sorted(used - set(workspace["dependencies"]))
    if missing:
        sys.exit(f"error: not in Zed's workspace dependencies: {', '.join(missing)}")

    deps = {
        dep: workspace_dependency(dep, workspace["dependencies"][dep], set(ours))
        for dep in sorted(used)
    }
    excluded = sorted(p for c, p in snapshot_crates().items() if c in DROPPED)

    lines = [
        "# Generated by scripts/apply_agentty.py from zed/zed-workspace.toml. Don't edit by hand:",
        "# change the script and rerun it.",
        "",
        "[workspace]",
        f'resolver = "{workspace.get("resolver", "2")}"',
        "members = [",
        *[f'    "{m}",' for m in members],
        "]",
        "exclude = [",
        *[f'    "{e}",' for e in excluded],
        "]",
        "",
    ]
    lines += toml_table(
        "workspace.package",
        {
            "version": VERSION,
            "edition": workspace["package"]["edition"],
            "publish": True,
            "repository": REPOSITORY,
        },
    )
    lines += toml_table("workspace.dependencies", deps)
    for name, table in workspace.get("lints", {}).items():
        lines += toml_table(f"workspace.lints.{name}", table)
    (ROOT / "Cargo.toml").write_text("\n".join(lines).rstrip("\n") + "\n")


def main() -> int:
    crates = snapshot_crates()
    ours = {c: p for c, p in crates.items() if c not in DROPPED}
    zed_deps = tomllib.loads(ZED_WORKSPACE.read_text())["workspace"]["dependencies"]
    for crate, path in sorted(ours.items()):
        edit_crate(crate, path, ours, zed_deps)
    write_root(ours)
    copy_apple_gpui_sources()
    print(f"› {len(ours)} crates, version {VERSION}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
