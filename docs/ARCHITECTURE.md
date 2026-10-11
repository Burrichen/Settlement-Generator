# Architecture

## Overview

```
                ┌───────────────────────────┐
                │      settlement-core      │   Rust library: all business logic
                │ models · dice · engine ·  │
                │ tags · content · storage  │
                │ interfaces · validation   │
                └────────────┬──────────────┘
                 ┌───────────┴────────────┐
       ┌─────────▼─────────┐    ┌─────────▼─────────┐
       │  settlement-gui   │    │  settlement-cli   │
       │  (src-tauri)      │    │  Ratatui TUI +    │
       │  thin IPC layer   │    │  CLI flags        │
       └─────────┬─────────┘    └───────────────────┘
                 │ Tauri IPC (invoke)
       ┌─────────▼─────────┐
       │  app/ (React+TS)  │   presentation only
       └───────────────────┘
```

**Rule:** business logic lives only in `settlement-core`. Both front ends are
thin adapters. If a front end needs a decision made (what to roll, how a tag
applies, whether content is valid), that decision is made by a core function.

## Repository layout

```
SettlementGenerator/
├── Cargo.toml              # Workspace root, holds the canonical version
├── package.json            # Frontend tooling (Vite, React, Tauri CLI)
├── vite.config.ts          # Vite root = app/
├── tsconfig.json
├── crates/
│   ├── settlement-core/    # Shared library (no UI, no Tauri, no terminal deps)
│   │   ├── src/
│   │   └── tests/          # Integration tests for the core and the repo
│   └── settlement-cli/     # Console binary `settlement-generator-cli` (Ratatui)
│       ├── src/
│       └── tests/
├── src-tauri/              # Tauri 2 shell, crate `settlement-gui`, binary `settlement-generator`
├── app/                    # React + TypeScript frontend source
├── content/                # Built-in (bundled) content data files
└── docs/                   # Canonical specifications
```

Changes from the suggested layout, and why:

- **No top-level `tests/` directory.** A top-level `tests/` is only compiled by
  Cargo if the workspace root is itself a package, which would blur the "core is
  the only shared library" boundary. Tests live with the crate they test
  (`crates/*/tests/`), and repository-wide invariants such as version
  single-sourcing are tested from `settlement-core/tests/`. Running
  `cargo test --workspace` runs all of them.
- **`src-tauri/` is at the repository root** rather than inside `crates/`. This
  is the location the Tauri CLI expects by convention. It is still a normal
  workspace member.
- **Frontend tooling (`package.json`, `vite.config.ts`) is at the root**, with
  source in `app/`. This lets the Tauri CLI and npm scripts run from one
  directory.

## Crates

### `settlement-core` (library)

This crate owns, or will own:

| Area          | Responsibility                                                                                                                 | Status      |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------ | ----------- |
| Module        | Responsibility                                                                                                                 | Status      |
| ------------- | -------------------------------------------------------------------------------------                                          | ----------- |
| `app_info`    | Name and version (`APP_NAME`, `VERSION`, `app_info()`)                                                                         | Implemented |
| `ids`         | Stable ID newtypes, ID format, the reserved `user.` namespace                                                                  | Implemented |
| `dice`        | Parse dice expressions and roll them die by die (`FaceSource`: RNG or physical entry)                                          | Implemented |
| `content`     | Tables, results, variants, conditions, tags and effects as data                                                                | Implemented |
| `validation`  | Content integrity checks with structured, located issues                                                                       | Implemented |
| `engine`      | Roll tables, apply tag effects, follow routes, and record every die                                                            | Implemented |
| `session`     | Stage-by-stage generation: Automatic, Incremental, One-table; dependency re-checks                                             | Implemented |
| `editor`      | Editor support: dice coverage maps, tag usages, effect schema, relevant issues                                                 | Implemented |
| `service`     | `AppService`: the application service both interfaces call (settings, content editing, generation sessions, saved settlements) | Implemented |
| `naming`      | Naming schemes and methods as data; deterministic name joining (compound, possessive, exceptions)                              | Implemented |
| `layers`      | Merge bundled defaults with user overrides; edit, delete and reset operations                                                  | Implemented |
| `bundled`     | Bundled default content (`content/*.json`, embedded by `build.rs`)                                                             | Implemented |
| `settings`    | Settings model: generation method/presentation, God Mode, Tag View, theme                                                      | Implemented |
| `settlements` | Saved settlement snapshots and summaries                                                                                       | Implemented |
| `db`          | SQLite database: transactional migrations, settings, overrides, settlements                                                    | Implemented |
| `backup`      | Verified database backups before migrations and before installing updates                                                      | Implemented |
| `storage`     | User data directory and default database location                                                                              | Implemented |

Constraints:

- No dependency on Tauri, React, Ratatui or crossterm.
- Public types that cross the IPC boundary derive `serde::Serialize` and
  `Deserialize`, so the GUI receives exactly the same structures the TUI uses.
- The source of dice faces is injectable (`dice::FaceSource`). Tests use seeded
  or scripted dice, and users can enter physical rolls the same way.
- Edits to content only ever change the user override layer. Bundled defaults
  are read-only (see DATA_MODEL.md §6).

### `settlement-gui` (`src-tauri/`)

- A Tauri 2 application. The Rust side only exposes `#[tauri::command]` wrappers
  that call core functions and return core types.
- The binary is `settlement-generator.exe`. It uses the Windows GUI subsystem in
  release builds, so no console window appears.
- The frontend (`app/`) is presentation only. It calls commands through a single
  typed wrapper module (`app/src/api.ts`), so IPC names are kept in one place.
- **State:** the `AppService` (or the error that stopped it opening) is
  created once at startup and held in Tauri managed state. The current
  generation or edit session is held there too.
- **Commands:**
  - `app_info`, `roadmap`, `get_settings`, `save_settings`, `exit_app`;
  - `generation_*`: `start`, `view`, `roll`, `choose`, `clear`, `advance`,
    `back`, `roll_remaining`, `run_automatic`, `reroll_all`, `discard` and
    `save`.
  - `content_*`: `overview`, `table`, `tag`, `check_table`, `check_tag`
    (live draft validation and coverage), `save_table`, `save_tag` (checked
    edits), `new_table`, `new_tag`, `new_id`, `delete_*`, `reset_*` and
    `reset_all`; plus `effect_schema`.
  - `saved_*`: `list`, `get`, `set_favourite`, `delete` (refused by the core
    for favourites), `edit_start` (restores a snapshot into an editing
    session) and `edit_save` (saves an explicit revised snapshot).
  - `update_check` and `update_install` (`updates.rs`):
    - signed updates from GitHub Releases through the Tauri updater plugin;
    - only stable, newer versions are offered;
    - installing flushes and backs up user data first, then closes the
      database;
    - see [RELEASING.md](RELEASING.md).

  Generation actions return the new `SessionView` plus a plain-language
  error when an action could not be completed. Content is the bundled
  defaults merged with the user's overrides from the database. Physical dice
  come from `RandomFaces::from_entropy()`.

- **Content editors** (`TagsScreen`, `TablesScreen`) are generic: one form for
  every table, with effect forms built from the core's effect schema. They
  edit a whole draft, show the core's live validation and coverage, and save
  only when the core accepts the draft.
- **Generation screens** (`StartScreen`, `CompletedSettlement`) only render
  the `SessionView` and send the user's actions to the core. Legal results,
  dice and dependency rules all come from the core.
- **Updates and data errors:** `App.tsx` checks for an update at launch.
  When the Main Menu is showing, `UpdatePrompt` offers Update or Later.
  If the user database cannot be opened (for example after a failed
  upgrade), a "Your data could not be opened" view shows the core's
  recovery message instead of the menu.
- **Navigation:** a screen stack in `App.tsx`. `Esc` goes back from any
  screen, and the main menu restores focus to the last item used.
- **Theming:** every colour, font and ornament is a CSS variable in
  `app/src/styles/tokens.css`, with one `[data-theme]` block per theme
  (`modern-dark`, `old-timey-dark`). A new theme is a new block. Fonts (Inter,
  Cormorant Garamond) are bundled through `@fontsource`, so the app needs no
  network access.
- **Keyboard:** everything is a native button, radio or checkbox. The main
  menu adds arrow-key, Home and End navigation. Disabled destinations are not
  focusable.
- **Tests:** `app/src/*.test.tsx` uses Vitest with Testing Library, and
  mocks the `api` module at the IPC boundary.
- TypeScript types for core structures are currently written by hand to mirror
  the Rust structs. If the model grows large, generating them (for example with
  `ts-rs` or `specta`) should be considered. This is deferred.

### `settlement-cli` (`crates/settlement-cli/`)

- A Windows console binary, `settlement-generator-cli.exe` (crate `settlement-cli`). It is a separate executable from the GUI, so the GUI keeps the Windows GUI subsystem (no console window) and the TUI gets a real console.
- With no arguments it launches the full-screen Ratatui TUI. Standard flags
  (`--version`, `--help`) are handled by `clap`, and the version string comes
  from the core.
- Uses Ratatui's crossterm backend, which works in Windows Terminal, conhost and
  Unix terminals. Only `KeyEventKind::Press` events are handled, because Windows
  also reports key releases.
- A full application with feature parity with the GUI. It calls the same
  `AppService` and uses the same database:
  - Main Menu, Settings, Roadmap and Credits;
  - Start in Automatic, Incremental and One-table modes, the completed
    settlement, and editing;
  - View, split into list and detail when the terminal is at least 80 columns
    wide, and stacked otherwise;
  - stepped tag and table editors, including a condition-tree editor and
    schema-driven effects.
- **Structure:** a stack of `Screen`s (`src/tui/screens/`) plus shared
  widgets: selectable row lists, a text input, and modals for pick,
  checklist, confirm and message.
- **Look:** hierarchy through bold and dim, one accent colour per theme, and
  the terminal's own foreground colour for body text.
- **Small windows:** below 48×14 the app shows a "Window too small" notice
  instead of drawing.
- **Tests** drive the real screens with key presses against an in-memory
  database and render them to Ratatui's `TestBackend` at several sizes.

## Data and storage

- **Built-in content** is in `content/` and is bundled with both binaries. It is
  read-only at runtime.
- **User data** (content customisations, settings and saved settlements) lives
  in one SQLite database in the per-user OS data directory:
  `%APPDATA%\SettlementGenerator\settlement-generator.sqlite3` on Windows.
  - The core computes this location and owns all access, so both front ends
    share the same data.
  - It is never written inside the install directory, so updates cannot touch
    it.
  - SQLite was chosen over loose JSON files because it gives transactions,
    multi-process safety and versioned migrations in one place.
- Both interfaces may run at the same time. The database uses WAL mode and a
  busy timeout, and every multi-step write is a transaction.
- Schema changes are applied by numbered migrations.
  - All pending migrations run in one transaction, after a verified backup.
  - A failed upgrade changes nothing and keeps the backup (see DATA_MODEL.md
    §9 and [RELEASING.md](RELEASING.md)).

The format details are in [DATA_MODEL.md](DATA_MODEL.md).

## Networking

The only network access is the GUI update check and download. This is the
Tauri updater plugin reading the latest GitHub Release, with signed updates
(see [RELEASING.md](RELEASING.md)). There is no telemetry, analytics or remote
storage.

## Cross-platform stance

Windows is the only supported target for v0.1.0. To avoid locking out
macOS/Linux:

- Paths are built with `std::path` and OS data directories, never hardcoded
  `C:\` paths or `\` separators.
- Windows-specific attributes are guarded with `cfg` (for example
  `windows_subsystem`).
- The TUI uses crossterm, which is cross-platform.
- The Tauri bundle configuration lists only Windows targets today. Adding other
  targets is a configuration change.

## Quality gates

These are run for every change (see the root `README.md` for the commands):

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `npm run format`, `npm run typecheck`, `npm run build`, `npm test`
