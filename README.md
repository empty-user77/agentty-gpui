# agentty-gpui

The [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) UI framework as used by
[Agentty](https://github.com/empty-user77/Agentty): Zed's gpui crates, kept in step with Zed
releases, with the fixes and additions Agentty needs.

This is not an official Zed project. GPUI is made by Zed Industries and published under the
Apache License 2.0; see [LICENSE-APACHE](LICENSE-APACHE). Files changed here say so in a
`Modified for Agentty` comment at the top.

## Branches

- **`upstream`** — Zed's files, unmodified: the gpui crates and the Zed workspace crates they
  depend on, at their Zed paths, one commit per Zed release
  ([`zed/SNAPSHOT.md`](https://github.com/empty-user77/agentty-gpui/blob/upstream/zed/SNAPSHOT.md)
  names the tag and commit).
- **`main`** — `upstream` plus Agentty's changes. This is what gets published.

## Updating to a new Zed release

```sh
scripts/sync-upstream.sh v1.29.0   # new snapshot on `upstream`
git merge upstream                 # into main; only files we changed can conflict
python3 -I scripts/apply_agentty.py
```

`scripts/apply_agentty.py` writes the root `Cargo.toml` and edits the crate manifests (our
package names, workspace version, no iOS / web, crates.io instead of Git). If a crate manifest
conflicts in the merge, take Zed's side (`git checkout --theirs <file>`) and rerun the script;
CI fails when its output isn't committed. Then build and test, and check Agentty against it
before publishing.

## Crates

Every crate is published as `agentty-gpui-<name>` (`gpui` itself as `agentty-gpui`), all at
the workspace version, and keeps Zed's library name, so code still says `use gpui::...`:

```toml
gpui = { package = "agentty-gpui", version = "0.1" }
```

Left out: `gpui_ios` and `gpui_web` (Agentty ships on macOS, Windows and Linux), and the
`image` / `image_gallery` examples, which need Zed's HTTP client.

## Dependencies

crates.io only takes crates.io dependencies. Where Zed builds from Git, we use the release:

| crate | Zed (v1.28.1) | here | what Zed's version adds |
|---|---|---|---|
| `async-task` | Zed fork | 4.7.1 | a fix for task waker vtable identity |
| `calloop` (Linux) | Zed fork | 0.14.5 | idle callbacks capped at about 10 ms per loop |
| `khronos-egl` (Linux) | Zed fork | 6.0.0 | no panic when a failed EGL call reports success |
| `wgpu`, `naga` (Linux) | Git, after 30.0 | 30.0.2 | newer API (`QueueDescriptor`, `has_srgb_suffix`) |
| `zed-font-kit`, `zed-scap`, `zed-xim` | Git | Zed's crates.io releases | small fixes since |
| `oo7` (Linux) | Zed fork | 0.6 | an error when the prompt stream ends |

## Files changed from Zed

Each carries a `Modified for Agentty` note, as do the crate manifests:

- `crates/gpui/src/style.rs` — its tests use `#[test]` instead of Zed's `#[perf]`.
- `crates/gpui_apple/build.rs` — reads the bundled `gpui-src/` copy when `../gpui` isn't there
  (a published crate); the script keeps that copy identical to `crates/gpui/src`.
- `crates/gpui_wgpu/src/{wgpu_context,wgpu_renderer,wgpu_atlas}.rs` — wgpu 30.0.2 API.

## Tests

```sh
cargo nextest run --workspace --features agentty-gpui-apple/runtime_shaders
cargo test --workspace --doc --features agentty-gpui-apple/runtime_shaders
```

Use nextest, as Zed does: a few tests read process-wide state and fail when `cargo test` runs
them side by side. Without `runtime_shaders`, macOS needs Xcode's Metal toolchain
(`xcodebuild -downloadComponent MetalToolchain`).
