# wml2viewer repo split memo

`wml2viewer` is now prepared to live outside the `wml2` workspace.

## Current dependency shape

- `wml2viewer` depends on `wml2 = "0.0.19"`
- local development inside this repository uses:

```toml
[patch.crates-io]
wml2 = { path = "../wml2/wml2" }
```

## Steps to create the new repository

1. Copy the contents of `wml2viewer/` into the root of the new repository.
2. Keep `Cargo.lock` committed because this is an application crate.
3. Move `.github/workflows/release.yml` with the repository contents.
4. Keep `LICENSE` in the new repository root.
5. If `wml2` should be consumed from crates.io, delete the `[patch.crates-io]` section in `Cargo.toml`.
6. If `wml2` should track an unreleased branch, replace the dependency with a `git` dependency instead.

## After the split

- remove `wml2viewer` from the old repository
- remove the root repository `viewer.yml` workflow
- update release tags and README links if the new repository name changes

## Verification

Run these commands in the new repository root:

```bash
cargo check
cargo build --release
```
