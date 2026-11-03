# Creating a release

1. Run the checks in the README.
2. Bump the package version in `Cargo.toml`, run `cargo check`, and commit the
   updated manifest and lockfile.
3. Move the Unreleased changelog entries under the new version and date.
4. Push an annotated version tag, for example `v0.26.0`.
5. Verify the release workflow and its Linux, Windows, Apple Silicon macOS and
   Intel macOS archives and checksums.

The tag workflow uses the pinned Rust toolchain. It also publishes to crates.io
when `CARGO_API_KEY` is configured and the account owns the package. On a fork,
leave that secret unset unless package publication has been arranged.

Distribution packages are maintained separately and may need explicit updates.
