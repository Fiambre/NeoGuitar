# NeoGuitar

Guitar Pro (GP3 to GP7) reader, viewer and player written in Rust.

```
NeoGuitar/
├─ crates/neoguitar-core/   parser, song model, playback engine, SoundFont synthesis, lyrics (no UI)
├─ crates/neoguitar-ui/     shared egui views: tablature, staff, fretboard, theme
├─ neoguitar-win/           Windows desktop app (egui), installer and updater
├─ neoguitar-android/       Android practice app (egui on NativeActivity); `android/build-apk.ps1 -Install`
├─ soundfonts/              GeneralUser GS, shared by every app
├─ samples/                 files used by the tests
├─ vendor/rustysynth/       patched copy of the synthesizer (accepts SoundFonts with empty instruments)
└─ .github/workflows/       CI and one release workflow per platform
```

Each platform is a thin app over `neoguitar-core`, so a fix in the parser or the synthesizer reaches all of them.
See [`neoguitar-win/README.md`](neoguitar-win/README.md) for the Windows app: features, build and downloads.

## Releases

Every platform is released with its own tag prefix, so versions do not collide:

| Platform | Tag | Workflow |
|---|---|---|
| Windows | `vX.Y.Z` (`win-vX.Y.Z` is accepted too; v0.3.0 can only read `vX.Y.Z`) | `release-win.yml` |

Releases: https://github.com/Fiambre/NeoGuitar/releases

## Development

```
cargo build --release --workspace
cargo test --release --workspace
```
## Optional tabs search (not in the store app)

- **Windows:** `More ▸ Find tabs online…` installs and uses the [gprotab](https://github.com/Fiambre/GTabs-Downloader) plugin (a separate program, checked by SHA-256).
- **Android:** `neoguitar-tabs-android` is a separate companion app, **NeoGuitar Tabs** (release tag `tabs-vX.Y.Z`, APK on GitHub only). NeoGuitar shows `Find tabs online…` in its open menu only when the companion is installed; the Play Store build contains no link to it.