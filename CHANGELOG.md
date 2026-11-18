# Changelog

All notable changes to Blurlist are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Every CI build also
carries a build number (the GitHub Actions run number) and the commit it was
built from, shown in **Settings → About** and in `blurlist --version`.

## [Unreleased]

## [0.1.0] - 2026-11-18

The first public release.

### Added

- **Opt-out roster** with one or more reference photos per person, groups
  (class, cabin, team) and notes. Pick the right face when a photo shows
  several people; get a warning when a reference photo is too small or blurry.
- **Folder import** for the roster: `Group/Full Name.jpg` becomes a person in a
  group. Photos that can't be added automatically are listed, never skipped.
- **Event scanning** of whole folders (recursively) with a whole-photo pass
  plus overlapping high-resolution tiles, so small faces at the back of group
  photos are found. EXIF orientation is applied before anything else.
- **Review screen** that shows every match side by side with the roster photo:
  `Y` accepts, `N` rejects, arrows move, `Space` shows the whole photo, `U`
  undoes. A filmstrip shows progress at a glance.
- **"Check these" pile** for uncertain matches and faces too small to
  identify. Photos with an unresolved face are held back from export — never
  blurred or skipped silently.
- **Photo viewer** with every face outlined, one-click blur/keep toggles, a
  blur preview and hand-drawn blur regions for anything the detector missed.
- **Re-match** an event against the current roster in seconds using stored
  face embeddings, keeping earlier decisions where nothing changed.
- **Export** to a new folder with an irreversible blur (soft blur, pixelate or
  solid fill), all metadata stripped (EXIF, GPS, XMP, IPTC, maker notes and
  embedded thumbnails), the colour profile kept, and no names written.
  Photos that changed after scanning are held back.
- **Models**: bundled YuNet detector (MIT) and SFace recogniser (Apache-2.0),
  run with ONNX Runtime. SCRFD and ArcFace models are supported but never
  bundled because their published weights are research-only. Custom ONNX
  models can be imported.
- **`blurlist` command-line tool** sharing the same database format.
- Installers for Windows (MSI, NSIS), macOS (Apple Silicon and Intel DMG) and
  Linux (AppImage, deb, rpm), with SHA-256 checksums.

[Unreleased]: https://github.com/lobojoel2003/Blurlist/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/lobojoel2003/Blurlist/releases/tag/v0.1.0
