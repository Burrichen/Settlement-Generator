# Versioning

## Application version

The application uses semantic versioning: `MAJOR.MINOR.PATCH`. The first release
is **v0.1.0**.

| Part  | Bump when…                                                                                                 |
| ----- | ---------------------------------------------------------------------------------------------------------- |
| PATCH | Fixes, compatibility work or polish, with no significant new user-facing capability.                       |
| MINOR | Backward-compatible feature or content expansion within the current scope of what the product generates.   |
| MAJOR | Significant expansion of generation scope, a fundamental schema change, or a major compatibility boundary. |

When displayed to users, the version is prefixed with `v` (for example `v0.1.0`).
The raw version string has no prefix.

### Roadmap

- **Next MINOR:** Improved name generation
- **Next MAJOR:** New tables

## Single source of truth

The version is defined in **exactly one place**:

```toml
# Cargo.toml (workspace root)
[workspace.package]
version = "0.1.0"
```

Everything else derives from it:

| Consumer                 | How it gets the version                                                                                                                                                     |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `settlement-core`        | `version.workspace = true`; exposed as `settlement_core::VERSION` (`env!("CARGO_PKG_VERSION")`)                                                                             |
| `settlement-cli`         | `version.workspace = true`; `--version` and the TUI read `settlement_core::VERSION`                                                                                         |
| `settlement-gui` (Tauri) | `version.workspace = true`; `tauri.conf.json` deliberately has **no** `version` key, so Tauri falls back to the Cargo version for the window, bundle and installer metadata |
| Updater                  | Tauri compares the Cargo version (the same `settlement_core::VERSION`, asserted by a test in `src-tauri/src/updates.rs`) with `latest.json` on the latest GitHub Release    |
| React frontend           | Asks the core over IPC (`app_info` command). `package.json` deliberately has **no** `version`.                                                                              |

The test `crates/settlement-core/tests/version_single_source.rs` fails if a second
hardcoded version appears in `tauri.conf.json` or `package.json`, or if any crate
stops inheriting the workspace version.

### Releasing a new version

1. Run `npm run version:bump -- X.Y.Z`. It edits
   `[workspace.package].version` in the root `Cargo.toml` and updates
   `Cargo.lock`, and it refuses versions that are not stable or not newer.
2. Run the full check suite (see the root `README.md`).
3. Build, sign and publish the release as described in
   [RELEASING.md](RELEASING.md). Releases must be stable `MAJOR.MINOR.PATCH`
   versions; installed apps never offer pre-release versions.

## Other version numbers (separate from the app version)

Data formats are versioned independently, because they change at a different
rate from the application:

- **Content schema version.** This is the format of built-in and user content
  files. A breaking content schema change implies a MAJOR app release.
- **Save format version.** This is the format of saved settlement files. Readers
  must keep loading every older save format that has been released (see
  [DATA_MODEL.md](DATA_MODEL.md)).

These are introduced together with the content and persistence code and are
recorded in the files they describe.
