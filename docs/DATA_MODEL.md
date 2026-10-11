# Data Model

> **Status:** implemented in `settlement-core`. This covers:
>
> - content: tables, results, variants, follow-up rolls, tags and effects;
> - the physical dice engine;
> - validation;
> - generation records and the stage-based generation session;
> - the default/override layering;
> - SQLite persistence of settings, customisations and saved settlements.
>
> This document describes the **format**. The actual v0.1.0 content is
> described in [CONTENT.md](CONTENT.md).

Source modules: `ids.rs`, `dice.rs`, `content.rs`, `validation.rs`,
`engine.rs`, `session.rs`, `layers.rs`, `bundled.rs`, `settings.rs`,
`settlements.rs`, `db/`, `storage.rs`.

## 1. Identifiers (`ids.rs`)

- Every addressable entity has an `id` that is separate from its `name`. IDs
  are immutable. Names and other display text are freely editable.
- References always use IDs, never names.
- Format: `^[A-Za-z0-9][A-Za-z0-9_.-]*$`. IDs are ASCII and
  case-sensitive, for example `world.lowFantasy`. Dots may be used for
  namespacing.
- Uniqueness scopes:

  | Kind           | Unique within |
  | -------------- | ------------- |
  | Table          | all tables    |
  | Tag            | all tags      |
  | Result         | its table     |
  | Variant        | its table     |
  | Follow-up roll | its table     |

- The `user.` prefix is **reserved for user-created items**.
  `layers::new_user_id(kind)` produces IDs such as
  `user.table.0192f3a4c1d2-7f3e09a1`. The time prefix keeps items in creation
  order. Built-in content never uses `user.`, so an update can never collide
  with a user's item.
- An ID that has been published in built-in content must never be reused for a
  different meaning.

## 2. Dice (`dice.rs`)

Supported dice: `d4`, `d6`, `d8`, `d10`, `d12`, `d20` and `d100` (percentile).

```
expression := term ("+" term)* (("+" | "-") constant)*
term       := count? "d" sides            count ≥ 1
```

Examples: `1d12`, `d20`, `2d6`, `1d8+1d4`, `2d6+1`. Whitespace is ignored and
`D` is accepted. The canonical display form always includes the count
(`1d20`). Limits: at most 20 dice per expression, and a constant modifier
within ±1000. Dice cannot be subtracted.

Rolling:

- An expression is parsed into individual dice. **Each die is rolled
  separately**, and the faces are summed with the modifier. There is never a
  hidden uniform pick over the total range, so `2d6` has the real bell curve.
- `d100` is simulated as a tens d10 and a units d10, with `00`+`0` read as 100.
- Faces come from a `FaceSource`:
  - `RandomFaces` is software dice with one uniform draw per die. It can be
    seeded for reproducible runs.
  - `ScriptedFaces` takes face values in order, for example entered from
    physical dice. It rejects values a die cannot show.
- A `DiceRoll` records the expression, every `DieRoll { die, value }`, the
  modifier and the total.

## 3. Content (`content.rs`)

A `ContentSet` is `{ schema_version, sequence, tables: [Table], tags: [Tag] }`.
It is stored as JSON. `CONTENT_SCHEMA_VERSION` is currently `1`.

`sequence` is the generation pipeline (see §10). It is an ordered list of
steps, each either a bare table ID or `{ "table": id, "name": "Region" }`. A
`name` labels the stage, and the label stays the same even when tags redirect
the stage to another table.

Fields marked † are optional, and their defaults are omitted when serialised.

### Table

| Field           | Meaning                                                                                                                                                             |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`, `name`    | Identity and display name                                                                                                                                           |
| `description` † | Display text                                                                                                                                                        |
| `dice` †        | Dice expression for the base layout. Required for rolled tables that are `final`                                                                                    |
| `selection` †   | `roll` (default; a result may also be chosen manually) or `choose` (always chosen, with no dice)                                                                    |
| `status` †      | `final` (default) or `incomplete`. Incomplete tables are reported by validation and stop generation unless a result is chosen. They must be resolved before release |
| `status_note` † | Why the content is not final                                                                                                                                        |
| `active` †      | Default `true`. Inactive tables are skipped                                                                                                                         |
| `eligibility` † | `Condition`. If false, the table is skipped                                                                                                                         |
| `results`       | Ordered results. Each result's `range` forms the **base layout**                                                                                                    |
| `variants` †    | Alternative layouts                                                                                                                                                 |
| `follow_ups` †  | Conditional follow-up rolls (see below)                                                                                                                             |

### Follow-up roll

A follow-up roll is a conditional extra roll made after the table's result is
determined, whether that result was rolled or chosen. It may change the
result. The Shinkira coastal upgrade uses one.

| Field                         | Meaning                                                                                                            |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `id`, `name`, `description` † |                                                                                                                    |
| `when` †                      | `Condition` on the active tags                                                                                     |
| `on_results`                  | Only applies when the current result is one of these                                                               |
| `dice`                        | Physical dice to roll                                                                                              |
| `outcomes`                    | `[{ range, becomes† }]`. These must cover every total exactly once. With no `becomes`, the original result is kept |

The final result's tags are emitted. The record keeps the original roll, the
follow-up roll and the change.

### Result

| Field           | Meaning                                                                                                                  |
| --------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `id`, `name`    | Identity and display text                                                                                                |
| `description` † |                                                                                                                          |
| `range` †       | Faces in the base layout: `[min, max]`, or a bare number for one face. Omitted means the result only appears in variants |
| `tags` †        | Tag IDs emitted when selected                                                                                            |
| `eligibility` † | `Condition`. If false when rolled, **reroll** (as at the table)                                                          |
| `enabled` †     | Default `true`. A disabled result keeps its faces, but rolling it means rerolling                                        |

### Variant (tag-driven alternative layout)

| Field                         | Meaning                                                                                                                          |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `id`, `name`, `description` † |                                                                                                                                  |
| `when` †                      | Selected automatically when this `Condition` holds (the first match wins)                                                        |
| `dice` †                      | Replacement dice. Defaults to the table's dice                                                                                   |
| `ranges` †                    | `[{ result, range }]` over the table's results. Defaults to the base ranges. Results not listed cannot be rolled in this variant |

### Condition

A condition is a JSON object with one key: `{"has_tag": id}`, `{"all": [..]}`,
`{"any": [..]}` or `{"not": cond}`.

### Tag

| Field           | Meaning                                                                       |
| --------------- | ----------------------------------------------------------------------------- |
| `id`, `name`    |                                                                               |
| `description` † |                                                                               |
| `effects` †     | List of effect definitions                                                    |
| `enabled` †     | Default `true`. Disabled tags are never activated and their effects never run |

### Effects

An effect definition is `{ "kind": …, ...params, "status"†, "note"† }`.

| `kind`             | Parameters                                 | Behaviour                                                                                                                                                                         |
| ------------------ | ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `route_to`         | `table`, `variant`†                        | Each time the tag is emitted, roll `table` next (depth-first), optionally forcing a variant                                                                                       |
| `use_variant`      | `table`, `variant`                         | While the tag is active, `table` uses this variant                                                                                                                                |
| `restrict_results` | `table`, `only`†, `exclude`†               | While the tag is active, only/excluded results of `table` are ineligible, so they are rerolled. `only` lists intersect and `exclude` lists combine                                |
| `modify_dice`      | `table`, `dice`†, `modifier`†              | While the tag is active, `table` is rolled with replacement dice and/or a modifier. A total past either end of the layout is read as that end                                     |
| `secondary_roll`   | `label`, `dice`                            | Each time the tag is emitted, roll `dice` and record it under `label`                                                                                                             |
| `add_tag`          | `tag`                                      | Each time the tag is emitted, also emit `tag`                                                                                                                                     |
| `redirect_table`   | `table`, `to`                              | While the tag is active, rolling `table` rolls `to` instead (for example a setting-specific Region table). Redirects chain; cycles are rejected                                   |
| `intent`           | `table`†, `results`†, `concept`†, `change` | An approved design intention without approved numbers (`increase`, `strong_increase`, `decrease` or `guarantee`). It is **never executable**: validation rejects an active intent |

- **Future modifiers targeting later tables:** every effect that names a
  `table` applies whenever that table is rolled later in the same generation
  while the tag is active.
- **Planned effects:** `"status": "planned"` stores an effect as design
  metadata. It is never executed, and its references (for example a table
  that does not exist yet) are not validated. Change it to `active` once the
  target exists.
- Unknown `kind` values are rejected when content is loaded.

## 4. Engine semantics (`engine.rs`)

`Generator::new(&content)` validates the content and refuses it if there are
any errors. Several entry points then resolve tables:

- `run(start_tables, faces)` rolls each start table in order.
- `run_with(..., selections, ...)` does the same, using chosen results.
- `run_sequence(selections, faces)` runs the whole pipeline.

Each table is resolved as follows:

1. **Redirects:** `redirect_table` effects from active tags, earliest
   activated first, replace the table. The record keeps `redirected_from`.
2. A table that is inactive, or whose eligibility fails, is skipped and the
   skip is recorded.
3. **Incomplete content:** if the table is `incomplete` and no result was
   chosen, generation **stops** with a readable error. Data is never invented.
4. **Choice:** if a result was chosen (`Selections`, keyed by the table
   actually used), it is checked and used without dice.
   - The choice is refused if the result is disabled, ineligible, restricted,
     or not on the current layout. God Mode lifts all of these except
     disabled.
   - A `choose` table with no choice is an error.
5. **Variant selection:** the variant forced by the route, otherwise the first
   `use_variant` from active tags (in activation order), otherwise the first
   variant whose `when` holds, otherwise the base layout.
6. **Dice:** the earliest `modify_dice` replacement wins. All modifiers are
   added together.
7. **Roll:** the dice are rolled, and the total is clamped to the layout's
   extent to read the face. If the face's result is disabled, ineligible or
   restricted, the table is rerolled. Every attempt is recorded.
   - If no eligible result can be reached, the error is `NoEligibleResult`.
     The engine does not loop forever.
   - The engine allows at most 1000 rerolls per table and 1000 table rolls
     per generation, which guards against runaway routing.
8. **Follow-up rolls** that apply are rolled in order and may change the
   result.
9. **Tags:** the final result emits its enabled tags. Emission follows
   `add_tag` chains, with duplicates removed within one emission. It activates
   each tag and runs the per-emission effects (`route_to`, `secondary_roll`).

**God Mode** (`GenerationOptions { god_mode }`, set with
`Generator::with_options`) ignores tag-based result eligibility: result
`eligibility` conditions and `restrict_results` effects. Disabled results stay
disabled. Variants, dice changes and table-level eligibility still apply. The
chosen result still emits its tags.

**Records:** a `TableRollRecord` copies each table, variant, result and tag as
`{ id, name }` at the time of the roll. It also holds:

- the dice expression actually rolled;
- every attempt: the individual dice, total, face read, result and outcome;
- `input_tags`, the tags active when the table was resolved;
- `redirected_from`, `selected`, and the `follow_ups` records;
- `routed_by`, the tag whose `route_to` caused the roll;
- `modifiers`, each tag effect that shaped the roll (variant, dice or
  restriction), stored as `{ tag, effect, ignored }`. `ignored` marks a
  restriction that God Mode overrode.

The `GenerationLog` also records the options it ran with. Its `Display`
output is:

```
table: Economy
dice: 1d12
roll: 9
result: Prosperous
```

## 5. Validation (`validation.rs`)

`validate(&ContentSet) -> ValidationReport` never panics. Each issue has a
`severity` (`error` or `warning`), a `location` (table, variant, result, tag
and effect index), a machine-readable `code` with data, and a `message`. Both
interfaces show the message as it is.

| Code                         | Severity | Detects                                                                                       |
| ---------------------------- | -------- | --------------------------------------------------------------------------------------------- |
| `unsupported_schema_version` | error    | A content or override file this build cannot read                                             |
| `invalid_id`                 | error    | An ID that does not match the ID format                                                       |
| `duplicate_id`               | error    | A duplicate table, tag, result, variant, or variant range assignment                          |
| `invalid_dice`               | error    | A bad expression in a table, variant, `modify_dice` or `secondary_roll`                       |
| `impossible_range`           | error    | A range with `min > max`, or faces the dice cannot produce                                    |
| `overlapping_ranges`         | error    | Two results claiming the same face                                                            |
| `uncovered_outcomes`         | error    | Possible totals with no result                                                                |
| `missing_tag`                | error    | A reference from result tags, conditions or `add_tag`                                         |
| `missing_table`              | error    | An effect targeting a table that does not exist                                               |
| `missing_variant`            | error    | `use_variant`, or a `route_to` variant, that does not exist                                   |
| `broken_result_reference`    | error    | Variant ranges or `restrict_results` naming an unknown result                                 |
| `routing_cycle`              | error    | Tables that route into each other through **every** rollable result, so generation never ends |
| `routing_cycle`              | warning  | Tables that **can** route into each other (bounded by the step limit)                         |
| `unreachable_result`         | warning  | A result with no faces in any layout                                                          |
| `not_user_id`                | error    | A user-created item outside the `user.` namespace                                             |
| `override_target_missing`    | warning  | A customisation of a default that no longer exists. It is ignored                             |
| `default_changed`            | warning  | A default that was updated after the user customised it                                       |
| `missing_dice`               | error    | A rolled, final table with no dice expression                                                 |
| `incomplete_content`         | warning  | A table marked `incomplete`, shown with its `status_note`. It must be resolved before release |
| `intent_not_planned`         | error    | An `intent` effect marked active                                                              |
| `redirect_cycle`             | error    | Tables that redirect into each other                                                          |

Range checks run on:

- the base layout;
- each variant that overrides dice or ranges;
- each follow-up roll's outcomes.

They are skipped for `choose` tables and for incomplete tables that have no
dice. Sequence steps must name existing tables.

## 6. Content layers (`layers.rs`)

```
effective content = merge(bundled defaults, user overrides)
```

**Bundled defaults** come from every `content/*.json` file.

- Each file is a partial `ContentSet`. The files are merged in file-name
  order, and only one of them may define `sequence`.
- The build script compiles the files into every binary, so the defaults ship
  with, and change with, the application version.
- They are never written at runtime.

**User overrides** (`UserOverrides`) are stored as one JSON document in the
`content_overrides` table of the application database (§9). If none is stored,
there are no customisations. A corrupt document, or one from a newer format,
gives a clear error.

```jsonc
{
  "format_version": 1,
  "tables": {
    "<bundled id>": { "action": "modify", "base": "<fingerprint>",
                      "name"?, "description"?, "dice"?, "active"?, "eligibility"?,
                      "results": { "<bundled result id>": { "action": "modify", "base": "…", "name"?, "range"?, … }
                                   | { "action": "delete" } },
                      "added_results": [ <Result with user. id> ] },
    "<bundled id>": { "action": "delete" },
    "user.…":       { "action": "custom", …full Table… }
  },
  "tags": { "<bundled id>": { "action": "modify", … } | { "action": "delete" },
            "user.…": { "action": "custom", …full Tag… } }
}
```

- **Field-level patches.** A customised table only overrides the fields the
  user changed. Everything else, including new bundled results, still comes
  from the installed defaults. Setting a field back to its default value
  removes it from the patch, and an empty patch is removed entirely.
  `null` means "explicitly cleared" (for example removing a range), which is
  different from "not overridden".
- **Deleting a bundled item** writes a `delete` marker. Bundled files are
  never touched. **Deleting a user item** removes it.
- **Disabling** uses the content's own `active` and `enabled` flags, through
  patches.
- **Reset** removes the override for one table, result or tag, or for
  everything (`reset_all`, which also removes user-created items). The item
  then falls back to the currently installed defaults.
- **Updates:** when the defaults change, overrides stay intact and new
  untouched defaults appear. Each patch stores a `base` fingerprint (FNV-1a of
  the default's canonical JSON). If the default has changed since, merging
  reports a `default_changed` warning, and the user's values still win. An
  override whose default was removed is ignored, with an
  `override_target_missing` warning.
- **Variants and follow-up rolls** of a bundled table can be customised. Each
  is overridden as a whole list (`variants` and `follow_ups` in the patch).
  The selection mode, content status and bundled result order of a built-in
  table cannot be changed.
- **Whole-item edits:** the editors send a complete draft.
  `put_table` and `put_tag` turn the draft into the smallest patch against
  the default. Unchanged fields stay default, removed bundled results become
  `delete` markers, and new results need `user.` IDs.
- **Checked edits:** `apply_checked(edit)` accepts an edit only if it adds no
  new validation errors anywhere in the merged content. Otherwise nothing
  changes and the new errors are returned. A malformed table, or deleting a
  tag that is still referenced, therefore never reaches generation.
  `preview(edit)` evaluates a draft without storing it, which is how the
  editors give live feedback.
- **Editor support** (`editor.rs`):
  - `coverage(table)`: for the base layout, each overriding variant and each
    follow-up roll, every possible total, which results claim it, and its
    exact chance under the physical dice. Uncovered, overlapping and
    impossible faces are listed.
  - `tag_usages`: where a tag is emitted, checked or added.
  - `relevant_issues`: the issues for the item being edited, plus any errors
    an edit newly causes elsewhere.
  - `effect_schema()`: every effect kind with typed fields. Interfaces render
    effect forms from this, and a test keeps it in step with the `Effect`
    enum.
- `merge` never fails. The resulting `MergedContent` holds the content, the
  origins of each item (`bundled`, `modified` or `custom`, plus deleted
  defaults), and a report covering layering issues and full validation.
  `MergedContent::generator()` refuses to generate while there are errors, so
  invalid customisations produce a validation error instead of a crash.

## 7. Saved settlements (`settlements.rs`)

A saved settlement is a self-contained **snapshot**:

| Field                      | Meaning                                                                                                                                   |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                       | Stable random ID (`stl-` + 32 hex digits)                                                                                                 |
| `name`                     | Entered by the user. It is trimmed and cannot be empty                                                                                    |
| `created_at`, `updated_at` | UTC, millisecond precision. JSON uses RFC 3339; the database stores Unix ms                                                               |
| `app_version`              | Application version that generated the current snapshot                                                                                   |
| `favourite`                | Boolean                                                                                                                                   |
| `snapshot.log`             | The full `GenerationLog` (§4)                                                                                                             |
| `snapshot.stages`          | Each pipeline stage as saved: its label, stage table, the table actually used, the result, the tags activated, and whether it was skipped |

The generation log holds the table results, the labels visible at generation
time, the tags (with names at that time), the recorded dice, routing, applied
modifiers and the God Mode flag.

- A snapshot is displayed from its own copies and is **never re-resolved**
  against current content. Later edits to tables or tags, and application
  updates, cannot change it.
- **Explicit changes only.** `rename_settlement` changes the name and
  `updated_at`. `replace_snapshot` (used after a deliberate edit or reroll)
  replaces the snapshot and sets `updated_at`. It also sets `app_version` to
  the version that produced the new snapshot. `created_at` never changes.
- **Display** uses `SettlementSnapshot::display_stages()`. This returns the
  recorded stages, or derives them from the log for snapshots saved without
  them. It never looks at current content.
- **Editing** uses `GenerationSession::from_snapshot`.
  - The saved records are restored stage by stage, exactly as saved.
  - Dependency contexts are measured against the current content when
    editing begins. A stage that cannot be measured is kept as saved.
  - Only changes the user makes can therefore invalidate later stages.
    Changed defaults never regenerate anything.
  - Untouched stages are written back verbatim by `to_snapshot()`.
  - `Database::update_settlement` saves the new name and the revised
    snapshot in one transaction.
  - `app_version` is replaced only when the snapshot itself changed. A
    rename alone keeps the version that generated the results.
  - Leaving an edit asks first only when there are unsaved changes, in both
    interfaces. The terminal interface asks the core
    (`AppService::edit_has_changes`); the GUI tracks the name and stage
    changes made since the edit opened.
  - `AppService::save_edit` refuses to save an edit with nothing changed.
- **Favourite/delete invariant:** a favourite cannot be deleted. It must be
  unfavourited first. `Database::delete_settlement` enforces this and returns
  `FavouriteCannotBeDeleted`. A database trigger also blocks the deletion, even
  for raw SQL.
- Each row stores `snapshot_format` (currently 1). A snapshot that is newer or
  corrupt gives an error for that settlement only. Listing still works.

## 8. Settings (`settings.rs`)

| Setting                   | Values                          | Default       |
| ------------------------- | ------------------------------- | ------------- |
| `generation_method`       | `automatic`, `manual`           | `automatic`   |
| `generation_presentation` | `incremental`, `one_table`      | `incremental` |
| `god_mode`                | boolean                         | `false`       |
| `tag_view`                | boolean                         | `false`       |
| `theme`                   | `modern_dark`, `old_timey_dark` | `modern_dark` |

- The presentation is only meaningful for Manual. While the method is
  Automatic it is **disabled completely**:
  - interfaces show it greyed out and not editable (`presentation_applies()`);
  - the core refuses to change it (`set_generation_presentation` and
    `Database::save_settings` return `PresentationDisabled`);
  - generation ignores it (`active_presentation()` returns `None`).

  Its saved value is kept, and switching back to Manual restores it.

- `god_mode` maps to `GenerationOptions` (§4) through `generation_options()`.
- `tag_view` controls whether interfaces show applied tags during generation
  and in saved settlements. The tags are always stored in the snapshot.
- Each setting is stored as its own key. A value this build cannot read (for
  example one written by a newer version) falls back to its default without
  affecting the other settings.
- **Changes, not copies.** Interfaces save through
  `AppService::change_settings(from, to)`:
  - `from` is what the interface was showing, and `to` is the user's
    change;
  - only the fields that differ are written, onto the settings as stored now;
  - so a change made meanwhile in the other interface (GUI or terminal) is
    never reverted by a stale copy.
- The installed version is not a setting: it comes from
  `settlement_core::VERSION`.

## 9. Database (`db/`)

- The database is a SQLite file at
  `%APPDATA%\SettlementGenerator\settlement-generator.sqlite3`
  (`storage::default_database_path`). It is in the per-user data directory,
  never beside the binaries. SQLite is compiled into the app (`rusqlite`
  `bundled`), so nothing needs to be installed.
- WAL journal mode and a busy timeout let the GUI and the terminal interface
  use the database at the same time.
- Tables: `settings` (key → JSON value), `content_overrides` (a single JSON
  document), and `settlements` (metadata columns plus the JSON snapshot).
- **Migrations:** these are numbered SQL files in `db/migrations/`.
  - **All pending migrations run in one transaction** together with their
    `PRAGMA user_version` bumps and a foreign-key check. An upgrade either
    completes, or leaves the database exactly as it was.
  - The transaction is `IMMEDIATE`, and the version is re-read inside it.
    If the GUI and the terminal interface open an old database together, one
    migrates and the other then finds nothing left to do.
  - Released migrations are never edited; schema changes add a new file.
  - A database from a newer application version (a higher `user_version`)
    is refused rather than misread.
- **Backups** (`backup.rs`) are written to `backups/` beside the database.
  - Each is a complete SQLite file made with `VACUUM INTO`, then re-opened
    and integrity-checked.
  - The backup file name is reserved with an exclusive create, so two
    processes can never write into the same backup.
  - Before migrating an existing database, `Database::open` writes a
    `pre-migration-…` backup. If it cannot, it does not migrate.
  - A failed upgrade returns `DbError::UpgradeFailed`, naming the backup.
  - Before an accepted application update, `AppService::prepare_for_update`
    flushes the database (`wal_checkpoint`) and writes a `pre-update-…`
    backup.
  - If the database could not be opened at all, for example after a failed
    upgrade, `AppService::prepare_unopened_for_update` backs up the file
    directly, read-only. An update that fixes the problem can then still be
    installed.
  - The newest 5 backups of each kind are kept.
  - See [RELEASING.md](RELEASING.md).

## 10. Generation sessions (`session.rs`)

A `GenerationSession` walks the content's `sequence`. Each step is a
**stage**, and the stage table may be redirected by tags.

| Mode          | Behaviour                                                                                                                                                                                                 |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `automatic`   | `GenerationSession::automatic` resolves every stage in order with physical dice. It takes `Selections` for `choose` stages such as Setting Profile, and stops with a readable error on incomplete content |
| `incremental` | Only the stage under the cursor can be rolled or chosen. `advance` requires the stage to be resolved. `back` moves the cursor back and keeps results until they are changed                               |
| `one_table`   | Every stage is shown and any stage can be rolled, rerolled, chosen or cleared. `roll_remaining` rolls every pending stage                                                                                 |

The mode comes from the settings (`GenerationMode::from_settings`).

**Dependencies.** Each resolved stage stores the context it was resolved
under:

- the table actually used (after redirects);
- the variant;
- the effective dice;
- the set of legal results;
- the follow-up rolls that apply to its result.

After any change, later stages are re-checked in order using the tags of the
stages before them:

- A stage whose context is unchanged is kept.
- A stage whose context changed is **invalidated**: it becomes pending, and its
  previous result is shown as `previous`.
- An invalidated stage no longer contributes tags, so the check cascades
  predictably.
- A skipped stage whose table would no longer be skipped is reopened.
- An action that fails (for example rolling an incomplete table, or choosing
  an illegal result) changes nothing.

**Legal results.** `StageView.options` lists every result of the table that
applies now, each with `legal` and a `reason` (`disabled`, `ineligible`,
`restricted` or `not_in_layout`). In **God Mode** every enabled result is
legal: eligibility conditions, restrictions and tag-selected layouts are
lifted. The chosen result still emits its tags, and downstream rules (for
example follow-up rolls) still apply.

**Views.** `ViewOptions { show_tags, debug }` controls how much is shown.
`ViewOptions::from_settings` sets `show_tags` from Tag View.

- **Basic views** contain the stage, the table, the status, the options and
  the result.
- `show_tags` adds the tags each stage emitted, and the tags each option would
  emit.
- `debug` adds `details`. This holds the input tags and the full records:
  variant, dice expression, every die, follow-up and secondary rolls,
  modifiers, final result and emitted tags. It is the reproducibility trail,
  and it is also stored in every saved snapshot.

`GenerationSession::log()` returns the settlement in pipeline order as a
`GenerationLog`, ready to save as a snapshot (§7).

**Interface support.** A session owns its content (`Arc<ContentSet>`), so a
front end can keep it between calls.

- `run_automatic(selections, faces)` resolves the pending stages in order. It
  stops at the first stage that needs the user (a choose-only stage, or
  incomplete content), keeping everything before it, and reports that stage
  as `next_pending`. The user then chooses a result for that stage, and
  `run_automatic` is called again to carry on.
- `reroll_all(faces)` rerolls the whole settlement from scratch. Choices that
  cannot be rolled (choose-only stages, and incomplete tables) are kept.
- `snapshot(ViewOptions)` returns a `SessionView`: the mode, God Mode, the
  cursor, whether the session is complete, `next_pending`, every
  `StageView`, and the active tags (only with `show_tags`).
- Each `StageView` also carries:
  - `dice`: the expression a roll would use now;
  - `outcome`: a short human-readable account, either "rolled 6 on 1d20" or
    "chosen", plus any follow-up rolls. This is separate from the full
    `details`.
