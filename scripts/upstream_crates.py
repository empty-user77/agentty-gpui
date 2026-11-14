"""List the Zed workspace crates that the given root crates need.

    python3 -I scripts/upstream_crates.py <zed checkout> gpui gpui_platform

Follows normal and build dependencies (every target) that point inside the Zed workspace,
by `workspace = true` or by `path`, and prints one `name<TAB>path<TAB>license` line per
crate. Dev-dependencies are left out: they pull in Zed's test tooling, not gpui.
"""

import pathlib
import sys
import tomllib


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    zed = pathlib.Path(sys.argv[1]).resolve()
    roots = sys.argv[2:]
    workspace = tomllib.loads((zed / "Cargo.toml").read_text())["workspace"]
    paths = {
        name: spec["path"]
        for name, spec in workspace["dependencies"].items()
        if isinstance(spec, dict) and "path" in spec
    }

    found: dict[str, tuple[str, str]] = {}
    todo = list(roots)
    while todo:
        name = todo.pop()
        if name in found:
            continue
        if name not in paths:
            print(f"error: {name} is not a path crate in the Zed workspace", file=sys.stderr)
            return 1
        crate_dir = paths[name]
        manifest = tomllib.loads((zed / crate_dir / "Cargo.toml").read_text())
        license_ = manifest["package"].get("license", "?")
        if isinstance(license_, dict):
            license_ = workspace.get("package", {}).get("license", "?")
        found[name] = (crate_dir, license_)

        tables = [manifest.get("dependencies", {}), manifest.get("build-dependencies", {})]
        for target in manifest.get("target", {}).values():
            tables += [target.get("dependencies", {}), target.get("build-dependencies", {})]
        for table in tables:
            for dep, spec in table.items():
                if dep in paths:
                    todo.append(dep)
                elif isinstance(spec, dict) and "path" in spec:
                    rel = str((zed / crate_dir / spec["path"]).resolve().relative_to(zed))
                    paths.setdefault(dep, rel)
                    todo.append(dep)

    for name, (crate_dir, license_) in sorted(found.items()):
        print(f"{name}\t{crate_dir}\t{license_}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
