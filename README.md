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
```

Then build and test, and check Agentty against it before publishing.
