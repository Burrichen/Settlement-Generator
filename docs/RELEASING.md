# Releasing, installing and updating

This document covers:

- what the Windows installer installs and where;
- how to build a release, with one command;
- how to bump the version;
- how releases are signed;
- the GitHub Releases checklist;
- how installed apps update, and how user data stays safe.

The first release is **v0.1.0**. The roadmap is unchanged:

- next minor update: Improved name generation (settlement naming itself
  shipped in v0.1.1);
- next major update: New tables.

## What the installer installs

Each release ships **one installer**:

- **File:** `Settlement Generator_X.Y.Z_x64-setup.exe`. This is an NSIS
  installer.
- **Install type:** per user. It does not need administrator rights.
- **Install location:** `%LOCALAPPDATA%\Settlement Generator\`.

| Installed item                          | Where                                                          |
| --------------------------------------- | -------------------------------------------------------------- |
| GUI: `settlement-generator.exe`         | Install folder. Start Menu and (optional) desktop shortcuts    |
| CLI/TUI: `settlement-generator-cli.exe` | Install folder, beside the GUI                                 |
| Bundled default generation content      | Compiled into both executables (from `content/*.json`)         |
| Updater configuration                   | Compiled into the GUI (`plugins.updater` in `tauri.conf.json`) |
| Uninstaller                             | Install folder, plus an entry in Windows "Installed apps"      |

Why two separate executables:

- The GUI uses the Windows GUI subsystem, so starting it never opens a console
  window.
- The CLI is a normal console program, so it works properly in Windows
  Terminal, PowerShell and cmd.
- Neither program pretends to be the other. Both call the same
  `settlement-core` and read the same version, content and database.

The bundled content is built into each executable, so there are no loose
content files to go missing or get out of date. Both programs always use
exactly the content they were built with.

### User data is outside the install folder

User data lives in `%APPDATA%\SettlementGenerator\`, which is roaming and
per-user:

- the database;
- its `backups/` folder.

The installer, updates and the uninstaller never touch this folder. The
uninstaller's optional "delete app data" checkbox removes only the WebView
cache folders named after the bundle identifier, so saved settlements survive
an uninstall and reinstall.

Tests check that the database and backup paths are outside the executable's
folder. See the `storage.rs` tests.

### Using the CLI

The installer puts the CLI beside the GUI, but it does **not** change `PATH`.
Editing `PATH` from an installer risks truncating a long user `PATH`, and
silent updates must not change system state. To use the CLI:

- **Run it by its full path** from any terminal:

  ```powershell
  & "$env:LOCALAPPDATA\Settlement Generator\settlement-generator-cli.exe"
  ```

- **Or add the install folder to your own `PATH`, once.** This changes the user
  `PATH` only, and takes effect in new terminals:

  ```powershell
  $dir = "$env:LOCALAPPDATA\Settlement Generator"
  $path = [Environment]::GetEnvironmentVariable("Path", "User")
  if (($path -split ";") -notcontains $dir) {
    [Environment]::SetEnvironmentVariable("Path", "$path;$dir", "User")
  }
  ```

  After that, `settlement-generator-cli` starts the TUI, and
  `settlement-generator-cli --version` prints the version.

- **To undo this**, remove the folder from the user `PATH` in Settings →
  System → About → Advanced system settings → Environment Variables.

Close any running CLI before updating or uninstalling. Windows cannot replace
a program file while it is running.

## Release build (one command)

```powershell
npm run release:build -- --notes path\to\notes.md
```

`scripts/build-release.mjs` does the following:

1. **Refuses to start** if any of these is true:
   - the version is not a stable `MAJOR.MINOR.PATCH`;
   - the updater public key or GitHub owner is still a placeholder;
   - the signing key is not in the environment;
   - the toolchain differs from the pinned one (`RELEASE_TOOLCHAIN` in
     `scripts/release-lib.mjs`: rustc 1.98.0 and Node.js 22).
2. **Starts clean.**
   - It deletes old bundles, the old upload folder and the staged sidecar.
   - It reinstalls npm packages exactly from `package-lock.json` (`npm ci`).
   - Every Cargo build uses `--locked`, so `Cargo.lock` is used exactly.
3. **Builds reproducibly.**
   - Incremental compilation is off.
   - The linker writes no timestamps (`/Brepro`).
   - Machine-specific paths are remapped (`--remap-path-prefix`).
   - With the same commit and the same pinned toolchain, the executables come
     out identical. This was verified for the CLI: two clean builds in
     different target folders gave byte-identical files. The GUI is built
     with the same flags.
4. **Builds the CLI** (`cargo build --release -p settlement-cli`). It then
   stages the CLI as the installer's sidecar,
   `src-tauri/binaries/settlement-generator-cli-<target>.exe`.
5. **Builds the GUI and the signed installer:**

   ```powershell
   tauri build --config src-tauri/tauri.release.conf.json
   ```

   - `tauri.release.conf.json` adds the CLI as an `externalBin`. It is kept out
     of the base config so that everyday `cargo` builds and tests do not need a
     prebuilt CLI.
   - The build fails if the signing key is missing.
   - After signing, Tauri checks the signature against the configured
     public key. A private key that does not match the app's `pubkey` stops
     the build, so a release can never be signed with a key the app would
     not trust.

6. **Checks the outputs.** Both executables, the installer and its signature
   must exist, and `settlement-generator-cli --version` must print the
   expected version.
7. **Writes the upload folder** `target/release/github-release/vX.Y.Z/`:
   - `Settlement.Generator_X.Y.Z_x64-setup.exe`, the installer under its
     GitHub asset name (GitHub turns spaces into dots);
   - `Settlement.Generator_X.Y.Z_x64-setup.exe.sig`, the updater signature;
   - `latest.json`, the updater manifest pointing at the assets above;
   - `SHA256SUMS.txt`, the checksums of the installer and the signature.

   It also writes `target/release/github-release/build-info-vX.Y.Z.json`,
   recording the exact tool versions used.

Other commands:

- `npm run release:manifest -- --notes notes.md` rewrites only `latest.json`,
  for example to fix the release notes.
- `npm run release:check -- --tag vX.Y.Z` checks readiness without building.
- `npm run release:build -- --packaging-test --test-pubkey <key.pub>` checks
  the packaging before the real key and owner are configured.
  - It needs a throwaway key pair generated outside the repository, for
    example:

    ```powershell
    npx tauri signer generate --ci -p test -w "$env:TEMP\sg-test\test.key"
    ```

  - The private key and password go in the usual environment variables.
  - Its public key is passed to the build through a temporary config overlay
    in `target/`; the committed configuration is never changed.
  - Its output folder is named `vX.Y.Z-PACKAGING-TEST-DO-NOT-PUBLISH`.
  - **Never upload it.** The app it contains cannot verify real updates.

What is not bit-for-bit reproducible:

- The NSIS installer embeds its own build time.
- The signature differs on every signing.

Checksums are therefore published for the exact files released, rather than
expected to match a rebuild. The executables inside are the reproducible part.

## Version bump procedure

The version is defined only in `[workspace.package]` of the root `Cargo.toml`
(see [VERSIONING.md](VERSIONING.md)).

```powershell
npm run version:bump -- X.Y.Z
```

- The command refuses anything that is not a stable version newer than the
  current one.
- It updates `Cargo.toml`, and updates the workspace entries in `Cargo.lock`
  (offline).
- The GUI, the CLI (`--version` and Settings → About), the installer file name
  and the updater all pick up the new version from there.

Choose the bump using the policy table in [VERSIONING.md](VERSIONING.md). Then:

- Run the full check suite (root `README.md`).
- Update the roadmap (`settlement-core/src/roadmap.rs` and PRODUCT_SPEC.md)
  only if the release delivers a roadmap item.
- Commit the change as "Release vX.Y.Z".

v0.1.0 is the initial version and needs no bump.

## Updater signing procedure

### Create the key pair (once)

Run this on the release machine, **outside the repository**:

```powershell
npx tauri signer generate -w "$env:USERPROFILE\.tauri\settlement-generator.key"
```

This writes two files:

- `settlement-generator.key` is the **private key**, encrypted with the password
  you choose. The project's current key was created **without a password**,
  so the file alone can sign updates. Keep it outside the repository and
  backed up.
- `settlement-generator.key.pub` is the public key.

Then configure `src-tauri/tauri.conf.json` under `plugins.updater`:

- Set `pubkey` to the **contents** of `settlement-generator.key.pub` (one base64
  line). This is public and belongs in the repository.
- In `endpoints`, replace `REPLACE_WITH_GITHUB_OWNER/settlement-generator` with
  the real `owner/repo`.

Leave these settings as they are. `tests/update_safety.rs` enforces them:

- `requireSignedVersion: true`
- no `dangerous*` options
- no `allowDowngrades`
- `bundle.createUpdaterArtifacts: true`

### Rules for the private key

- **Never commit it**, and never paste it into an issue, a log or a file in the
  repository.
- `.gitignore` excludes `*.key` and `.tauri/`.
- `tests/update_safety.rs` fails if a private key appears anywhere in the
  repository.
- Back up the key and its password offline, for example in a password manager.
  If either is lost, installed copies can no longer be updated automatically;
  users would have to reinstall by hand.
- Each released app trusts only the key it was built with. Rotating keys needs
  a transition release, signed with the old key, that carries the new public
  key.
- If CI is added later, use GitHub Actions secrets named
  `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, never
  workflow files.

### Signing a build

Load the key into the **current shell only**, just before building:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "$env:USERPROFILE\.tauri\settlement-generator.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Read-Host -MaskInput "Key password"
```

For the current passwordless key, set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
to an empty string instead.

`npm run release:build` signs the installer. Then clear the key:

```powershell
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Alternatively, close the shell.

## GitHub Releases checklist

Before building:

- [ ] All changes for the release are merged, and the working tree is clean.
- [ ] `npm run version:bump -- X.Y.Z` is done (skip for v0.1.0) and committed.
- [ ] The full check suite passes: `cargo fmt`, `clippy`, `cargo test`,
      `npm run format`, `npm run build`, `npm test`.
- [ ] The updater `pubkey` and GitHub owner are configured (once).
- [ ] Release notes are written (`notes.md`, plain text or Markdown).

Building:

- [ ] Load the signing key into the shell (see above).
- [ ] `npm run release:check -- --tag vX.Y.Z` reports "Ready".
- [ ] `npm run release:build -- --notes notes.md` succeeds.
- [ ] Clear the signing key from the shell.

Smoke test, on a Windows machine or VM:

- [ ] The installer runs without administrator rights. The Start Menu shortcut
      opens the GUI, and Settings → About shows `vX.Y.Z`.
- [ ] `settlement-generator-cli.exe --version` in the install folder prints
      `settlement-generator-cli X.Y.Z`, and running it with no arguments opens
      the TUI.
- [ ] When installing over the previous release, saved settlements, favourites,
      settings and content edits are all still there.

Publishing:

- [ ] Create the tag `vX.Y.Z` on the release commit and push it.
- [ ] Create a GitHub Release from that tag:
  - title `Settlement Generator vX.Y.Z`, with the notes as the description;
  - **not** a draft and **not** a pre-release, otherwise it never becomes
    `releases/latest`.
- [ ] Upload all four files from `target/release/github-release/vX.Y.Z/`,
      unchanged:
  - the installer;
  - its `.sig`;
  - `latest.json`;
  - `SHA256SUMS.txt`.
- [ ] Check that
      `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`
      shows the new version.
- [ ] Start the previous version. It offers the update. Choose Update, and the
      new version starts with all its data in place.

Test builds can be published as GitHub **pre-releases**. They never become
`releases/latest`, and the app ignores pre-release versions in any case.

## How updating works for users

1. When the GUI starts, a release build checks
   `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`.
   - This is the only network request the application makes.
   - If the check fails (for example when offline), nothing is shown.
   - Development (debug) builds never check, so they can never replace
     themselves.
2. If a newer **stable** version exists, the user is asked about it. A version
   with a pre-release suffix such as `-beta.1` is never offered.
   - The prompt shows the current version, the available version, and any
     release notes.
   - The choices are **Update** and **Later**. Escape means Later.
   - The prompt waits until the Main Menu is showing, so it never interrupts
     generation or editing.
   - Nothing is ever installed without the user choosing Update, and Later
     simply dismisses the prompt until the next launch.
3. When the user chooses **Update**, these steps run in order:
   1. The installer is downloaded and its signature is checked.
   2. The database is flushed (`wal_checkpoint`).
   3. A verified backup of the database is written. If the backup cannot be
      made, the update is **not** installed.
   4. The database is closed.
   5. The installer runs in passive mode. It replaces both executables, and
      the app restarts on the new version.
4. On the first launch of the new version, any pending schema migrations run
   (see below).

The commands that do this are `update_check` and `update_install` in
`src-tauri/src/updates.rs`. The webview gets no updater plugin permissions;
`tests/update_safety.rs` enforces this.

The CLI does not check for updates itself. The GUI's updates replace it,
because it is in the same installer.

## Data safety across updates

`%APPDATA%\SettlementGenerator\` contains:

- `settlement-generator.sqlite3`, which holds:
  - settings;
  - saved settlements and favourites;
  - table and tag overrides;
  - custom tables and tags.
- `backups/`, which holds verified backups:
  - `pre-update-<time>-v<from>-to-v<to>.sqlite3`, written before an accepted
    update is installed;
  - `pre-migration-<time>-schema<from>-to-schema<to>.sqlite3`, written before
    an existing database is migrated;
  - only the newest 5 of each kind are kept.

When an updated app opens the database, `Database::open` does the following:

1. It refuses a database from a _newer_ version and changes nothing.
2. If migrations are pending, it first writes a verified pre-migration backup.
   If that is impossible, it does not migrate.
3. It applies **all pending migrations in one transaction**, together with the
   `user_version` bumps and a foreign-key check. Either the whole upgrade
   commits, or the database is left exactly as it was.
4. If an upgrade fails, it returns `DbError::UpgradeFailed`. The error says that
   nothing was changed, names the backup, and tells the user how to recover:
   reinstalling the previous version opens the data as before.
   - The GUI shows this on a "Your data could not be opened" screen.
   - The CLI prints it and exits.

`crates/settlement-core/src/db/upgrade_tests.rs` simulates these upgrade paths
against a database holding every kind of user data:

- a successful upgrade that rebuilds a table;
- a failed migration, followed by recovery;
- a refused migration when no backup can be made;
- a downgrade attempt;
- pre-update backups and pruning.

Writing a new migration:

- Add `db/migrations/000N_name.sql` and append it to `MIGRATIONS`. Never edit a
  released migration.
- Rebuild tables in the standard SQLite way: create the new table, copy the
  rows, drop the old table, rename the new one, then recreate indexes and
  triggers. The favourite-protection trigger must be recreated.
- Add a simulated upgrade test from the previous released schema.
