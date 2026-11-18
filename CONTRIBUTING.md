# Contributing to Blurlist

Thanks for helping! Blurlist protects children's privacy, so correctness and caution matter more than speed.

## Ground rules

- **Never commit or attach real photos of children**, or of anyone who has not agreed. Tests use public-domain
  images (see `crates/blurlist-core/tests/fixtures/README.md`); UI work uses the illustrated demo.
- **No network access.** Blurlist must keep working fully offline and must not send anything anywhere.
- **Fail safe.** When unsure, hold a photo back or ask the user — never silently skip or guess.
- Only add models whose weights are licensed for production use, and document the licence.

## Setting up

```bash
npm ci
npm run fetch-models
npm run tauri dev        # desktop app
npm run dev              # UI only, in a browser, with the demo backend
```

On Linux install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) first.

## Before opening a pull request

```bash
npm run format:check && npm run lint && npm run typecheck && npm test
cargo fmt --all --check
cargo clippy --workspace --all-targets --features blurlist-core/cli -- -D warnings
BLURLIST_REQUIRE_MODELS=1 cargo test -p blurlist-core --features cli
```

If you changed the UI, regenerate the README images with `node scripts/screenshots.mjs` (needs Playwright).

## Commit messages and changelog

Write commit messages in the imperative ("Add HEIC support"). Add a line to the `[Unreleased]` section of
`CHANGELOG.md` for anything users will notice.

## Releasing (maintainers)

```bash
node scripts/version.mjs set X.Y.Z
# move the [Unreleased] notes into a new "## [X.Y.Z] - YYYY-MM-DD" section
git commit -am "Release X.Y.Z"
git tag -a vX.Y.Z -m "Blurlist X.Y.Z"
git push origin main vX.Y.Z
```

The release workflow verifies the tag against the version, builds every platform and publishes the release.
