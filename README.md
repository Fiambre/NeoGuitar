# NeoGuitar

Guitar Pro (GP3 to GP7) reader, viewer and player written in Rust.

```
NeoGuitar/
├─ crates/neoguitar-core/   parser, song model, playback engine, SoundFont synthesis, lyrics (no UI)
├─ neoguitar-win/           Windows desktop app (egui), installer and updater
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
| Windows | `win-vX.Y.Z` (older releases: `vX.Y.Z`) | `release-win.yml` |

Releases: https://github.com/Fiambre/NeoGuitar/releases

## Development

```
cargo build --release --workspace
cargo test --release --workspace
```