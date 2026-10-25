# Settlement Generator

Local-first Windows desktop app for generating tabletop settlements from
physical-dice tables. It has two interfaces, a Tauri GUI and a Ratatui terminal
UI, and both use the same Rust core.

The canonical specifications are in [`docs/`](docs/):

- [PRODUCT_SPEC.md](docs/PRODUCT_SPEC.md): what the product must do
- [ARCHITECTURE.md](docs/ARCHITECTURE.md): workspace layout and boundaries
- [DATA_MODEL.md](docs/DATA_MODEL.md): content, layering and save formats
- [CONTENT.md](docs/CONTENT.md): the bundled v0.1.0 generation content and its open items
- [NAMING_SYSTEM.md](docs/NAMING_SYSTEM.md): settlement naming: schemes, methods, dice rules and how to edit naming data
- [ORDON_NAMING.md](docs/ORDON_NAMING.md): the Ordon naming pack, its five methods, physical dice and editing workflow
- [MAPS.md](docs/MAPS.md): interactive settlement maps (design, storage, exports)
- [MAPS_GUIDE.md](docs/MAPS_GUIDE.md): how to use maps, in plain words
- [VERSIONING.md](docs/VERSIONING.md): version policy and its single source
- [RELEASING.md](docs/RELEASING.md): signed releases on GitHub, auto-updates and data safety
- [V0.1.0_RELEASE_CHECKLIST.md](V0.1.0_RELEASE_CHECKLIST.md): v0.1.0 status, deferred items, known limitations and manual release checks

## Prerequisites (Windows)

- Rust (stable, MSVC toolchain), with Visual Studio C++ build tools
- Node.js 22+ and npm
- Microsoft Edge WebView2 Runtime (preinstalled on Windows 11)

Run `npm install` once.

## Running

| What                             | Command                                    |
| -------------------------------- | ------------------------------------------ |
| GUI (dev, hot reload)            | `npm run tauri dev`                        |
| TUI (`settlement-generator-cli`) | `cargo run -p settlement-cli`              |
| CLI version                      | `cargo run -p settlement-cli -- --version` |
| Signed release (installer + CLI) | `npm run release:build` (see RELEASING.md) |
| Bump the version                 | `npm run version:bump -- X.Y.Z`            |

Release builds always go through `npm run release:build`. A plain
`npm run tauri build` would leave the CLI out of the installer.

## Checks (run before every change is considered done)

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run format
npm run build        # includes TypeScript type checking
npm test             # frontend tests (Vitest, jsdom)
```

Building the GUI crate requires `dist/` to exist, so run `npm run build` before
`cargo clippy`/`cargo test` on a fresh checkout.
