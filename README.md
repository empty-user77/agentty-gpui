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
before publishing (below).

## Using it

[![crates.io](https://img.shields.io/crates/v/agentty-gpui.svg)](https://crates.io/crates/agentty-gpui)

Every crate is published as `agentty-gpui-<name>` (`gpui` itself as `agentty-gpui`), all at
the workspace version, and keeps Zed's library name, so code still says `use gpui::...`. An app
needs two of them: `gpui`, and `gpui_platform` to start the backend for the OS it runs on.

```toml
[dependencies]
gpui = { package = "agentty-gpui", version = "0.1" }
gpui_platform = { package = "agentty-gpui-platform", version = "0.1" }
```

```rust
fn main() {
    gpui_platform::application().run(|cx: &mut gpui::App| {
        // open windows, bind keys, ...
    });
}
```

On Linux, turn on the `wayland` and / or `x11` features of both. On macOS, `runtime_shaders`
on `agentty-gpui-platform` compiles the Metal shaders at startup instead of at build time, so the
build doesn't need Xcode's Metal toolchain.

## Crates

| crate | Zed crate | what it is |
|---|---|---|
| [`agentty-gpui`](https://crates.io/crates/agentty-gpui) | `gpui` | the framework: elements, layout, windows, input |
| [`agentty-gpui-platform`](https://crates.io/crates/agentty-gpui-platform) | `gpui_platform` | picks the backend for the current OS |
| [`agentty-gpui-macos`](https://crates.io/crates/agentty-gpui-macos) | `gpui_macos` | macOS backend |
| [`agentty-gpui-apple`](https://crates.io/crates/agentty-gpui-apple) | `gpui_apple` | Metal renderer and text, shared by Apple platforms |
| [`agentty-gpui-windows`](https://crates.io/crates/agentty-gpui-windows) | `gpui_windows` | Windows backend |
| [`agentty-gpui-linux`](https://crates.io/crates/agentty-gpui-linux) | `gpui_linux` | Linux backend (Wayland, X11) |
| [`agentty-gpui-wgpu`](https://crates.io/crates/agentty-gpui-wgpu) | `gpui_wgpu` | wgpu renderer, used on Linux |
| [`agentty-gpui-macros`](https://crates.io/crates/agentty-gpui-macros) | `gpui_macros` | gpui's derive and attribute macros |
| [`agentty-gpui-util`](https://crates.io/crates/agentty-gpui-util) | `gpui_util` | small helpers |
| [`agentty-gpui-shared-string`](https://crates.io/crates/agentty-gpui-shared-string) | `gpui_shared_string` | `SharedString` |
| [`agentty-gpui-refineable`](https://crates.io/crates/agentty-gpui-refineable) | `refineable` | the `Refineable` trait behind style refinements |
| [`agentty-gpui-derive-refineable`](https://crates.io/crates/agentty-gpui-derive-refineable) | `derive_refineable` | its derive macro |
| [`agentty-gpui-collections`](https://crates.io/crates/agentty-gpui-collections) | `collections` | Zed's hash map and set types |
| [`agentty-gpui-sum-tree`](https://crates.io/crates/agentty-gpui-sum-tree) | `sum_tree` | the B-tree under lists and text |
| [`agentty-gpui-scheduler`](https://crates.io/crates/agentty-gpui-scheduler) | `scheduler` | async task scheduler |
| [`agentty-gpui-http-client`](https://crates.io/crates/agentty-gpui-http-client) | `http_client` | HTTP client interface (images from URLs) |
| [`agentty-gpui-zlog`](https://crates.io/crates/agentty-gpui-zlog) | `zlog` | logging |
| [`agentty-gpui-ztracing`](https://crates.io/crates/agentty-gpui-ztracing) | `ztracing` | tracing |
| [`agentty-gpui-ztracing-macro`](https://crates.io/crates/agentty-gpui-ztracing-macro) | `ztracing_macro` | its attribute macro |
| [`agentty-gpui-bench-metrics`](https://crates.io/crates/agentty-gpui-bench-metrics) | `bench_metrics` | performance counters |

Every platform's crates are published, though a build only compiles the ones for its target.
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
| `zed-font-kit`, `zed-scap` | Git | Zed's crates.io releases | small fixes since |
| `zed-xim` (Linux, X11) | Git | not used: its client half is `crates/gpui_linux/src/linux/x11/xim` | — |
| `oo7` (Linux) | Zed fork | 0.6 | an error when the prompt stream ends |
| `windows-capture` (Windows, `screen-capture`) | Zed fork of 1.4.3 | 1.4.4, held in `Cargo.lock` | — (1.5 no longer builds with `zed-scap`: a dependent enabling `screen-capture` must hold it too) |

## Files changed from Zed

Each carries a `Modified for Agentty` note, as do the crate manifests:

- `crates/gpui/src/style.rs` — its tests use `#[test]` instead of Zed's `#[perf]`.
- `crates/gpui_apple/build.rs` — reads the bundled `gpui-src/` copy when `../gpui` isn't there
  (a published crate); the script keeps that copy identical to `crates/gpui/src`.
- `crates/gpui_wgpu/src/{wgpu_context,wgpu_renderer,wgpu_atlas}.rs` — wgpu 30.0.2 API.
- `crates/gpui_linux/src/linux/x11/xim.rs` and `xim/` — the XIM client, from zed-xim 0.4.0-zed (MIT,
  `xim/LICENSE`), with a COMPOUND_TEXT decoder that knows the character sets Xlib writes and never
  fails: zed-xim panicked on the first Cyrillic, Greek or accented letter an input method committed.
  `x11.rs`, `x11/client.rs` and `x11/xim_handler.rs` use it.

## Tests

```sh
cargo nextest run --workspace --features agentty-gpui-apple/runtime_shaders
cargo test --workspace --doc --features agentty-gpui-apple/runtime_shaders
```

Use nextest, as Zed does: a few tests read process-wide state and fail when `cargo test` runs
them side by side. CI uses `--profile ci` (`.config/nextest.toml`), which retries a failed test twice:
a few bench tests time an 80 ms poll and can miss on a busy runner. Linux CI uses Mesa's software
Vulkan driver for the wgpu renderer tests. macOS CI skips `foreground_work_excludes_setup_before_trace_scope_starts`, which
needs a 10 ms sleep to return within 40 ms and doesn't on GitHub's virtualized Macs. Without `runtime_shaders`, macOS needs Xcode's Metal toolchain
(`xcodebuild -downloadComponent MetalToolchain`).

## Publishing

1. Bump `VERSION` in `scripts/apply_agentty.py`, rerun it (it writes the workspace version into
   the root `Cargo.toml`), and push. CI must be green, `cargo publish --workspace --dry-run` included.
2. Run the **Agentty** workflow (Actions → Agentty → Run workflow) with the Agentty branch to try:
   it builds and tests Agentty against this checkout on macOS, Windows and Linux.
3. `cargo publish --workspace` publishes the crates in dependency order. On a Mac without the
   Metal toolchain, add `--no-verify`: the verify build uses default features, and CI's dry run
   has already verified the same commit.
4. Tag the release (`git tag v0.1.0 && git push origin v0.1.0`).

crates.io limits new crates to a few at once and then about one every ten minutes, so the first
publish of all of them took a couple of hours; new versions of existing crates go through at once.
