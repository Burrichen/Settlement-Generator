# Settlement Generator — Product Specification

> Canonical product requirements. When a later prompt or decision changes a
> requirement, update this file in the same change.

- **Product name (working title):** Settlement Generator
- **First release:** v0.1.0 (see [VERSIONING.md](VERSIONING.md))
- **Platform:** Windows desktop, local-first. Nothing in the architecture should
  prevent future macOS/Linux builds.

## 1. Purpose

A desktop tool that generates tabletop-game settlements from random tables. Every
roll must be reproducible with physical dice, so a game master can use the app,
the printed tables, or both together.

The actual generation content (which tables exist, their entries, tags and how
they combine) **has not been specified yet** and will come in later prompts.
Nothing in this document defines game content.

## 2. Interfaces

The application ships with **two complete interfaces**:

| Interface | Technology                          | Audience                                  |
| --------- | ----------------------------------- | ----------------------------------------- |
| GUI       | Tauri 2 + React + TypeScript + Vite | Mouse/keyboard desktop users              |
| TUI / CLI | Rust + Ratatui (Windows console)    | Keyboard-only users, terminal power users |

Requirements common to both:

- Both use the **same** generation engine, content, settings and saved
  settlements, all provided by the shared Rust core crate.
- No business logic is implemented in either interface. They only present data
  and forward user intent to the core.
- Both can display the installed application version. The GUI Settings page must
  eventually show it.
- The TUI must be fully usable from the keyboard.

## 3. Generation principles

1. **Data-driven content.** Tables, tags, table results, tag effects and
   generation rules are data loaded at runtime. They are not hardcoded into UI
   components or engine code. Assume nearly all content will change in future
   versions.
2. **Physical-dice reproducibility.** Every random table declares a dice
   expression made from real tabletop dice:
   - d4, d6, d8, d10, d12, d20
   - d100 (percentile, i.e. two d10s)
   - sensible combinations of those dice (for example `2d6` or `1d8+1d4`)

   Software rolls simulate the declared expression die by die. They never
   silently substitute an arbitrary percentage or uniform pick.

3. **No invented content.** The application only contains content that has been
   explicitly specified.

## 4. Identity, customisation and persistence

- Every table, result, tag and rule has a **stable internal ID** that is separate
  from its editable display name. Renaming something never breaks references.
- **Built-in content and user modifications are stored separately.** Application
  updates may change bundled defaults but never overwrite user customisations.
- **Saved settlements are snapshots.** Later changes to tables or tags (by the
  user or by an update) never silently change an existing saved settlement.

## 5. Operating constraints

- Works completely offline. The only network access ever permitted is checking
  for and installing application updates.
- No cloud accounts, analytics, telemetry or remote storage.
- Features that were not requested are not added. Extension points are allowed
  when they keep future work cheap, but speculative systems (for example DLC or
  plugin frameworks) are not built ahead of need.

## 6. Roadmap

| Version    | Scope                                          |
| ---------- | ---------------------------------------------- |
| v0.1.0     | First release (scope defined by later prompts) |
| v0.1.1     | Settlement name generation                     |
| v0.1.5     | Authority, settlement state, Notable Locations |
| Next MINOR | Improved name generation                       |
| Next MAJOR | New tables                                     |

## 7. Customisation requirements

The user can, for both built-in and user-created content:

- rename tables, tags and results;
- change dice expressions, result ranges and tag effects;
- add tables, tags and results;
- disable built-in items, and delete built-in or custom items;
- restore one item, or all generator content, to the installed defaults.

Built-in items are never physically modified. Every customisation is recorded
in the user override layer (see [DATA_MODEL.md](DATA_MODEL.md) §6). Invalid
customisations produce clear validation errors and never crash generation.

## 7a. Content editors

Settings → View Tags and View Tables list every effective tag and table
(defaults merged with customisations). The editors are generic, with no
form written for a specific table.

- **Tags:** the ID is shown read-only. You can edit the name and description,
  enable or disable the tag, and edit its effects (forms come from the effect
  schema). You can also see where the tag is used, add or delete custom tags,
  reset a customised default, and restore a deleted default.
- **Tables:**
  - Edit the name, description, dice, active state and table eligibility.
  - Edit results: faces, name, emitted tags, eligibility, enabled; add or
    remove them.
  - Edit tag-driven variants (condition, dice, ranges) and follow-up rolls.
  - Add or delete custom tables, reset a customised default, and restore all
    defaults.
- **Physical-dice validity is visible:** every possible total is drawn as a
  strip, uncovered faces are hatched and overlaps highlighted, and each
  result shows its exact chance.
- **Nothing invalid is accepted:** changes are checked live, and Save stays
  disabled while there are errors. The core also refuses any save that would
  make the content invalid, such as deleting a tag that is still used.
  Renaming is always safe because references use IDs.
- **God Mode** exposes every enabled result of the current table, whatever
  its eligibility. It never exposes disabled or deleted results, or content
  that fails validation.

## 8. Settings

Settings are persistent and shared by both interfaces:

- **Generation Method:** Automatic or Manual.
- **Generation Presentation:** Incremental or One-table.
  - This only applies to Manual. While Automatic is selected, the setting is
    **disabled completely**: it is greyed out, it cannot be changed (the core
    enforces this) and it is ignored.
  - Its saved value is kept, and switching back to Manual restores it.
- **God Mode** (on/off): tag-based result eligibility restrictions are
  ignored, so every result of a table is visible and selectable. The tags
  emitted by the chosen result are still applied.
- **Tag View** (on/off): shows applied tags during generation and when viewing
  saved settlements.
- **Bare Minimum Buildings** (on/off, off by default): every settlement gets
  exactly one extra Notable Location, a Tavern with 4 spare rooms. It is
  added to the rolled Notable Locations, never instead of one, and is never
  repeated per district.
- **Theme:** Modern Dark (the default) or Old-Timey Dark.

## 9. Saved settlements

- A saved settlement keeps a snapshot of what was generated at that moment:
  - a stable ID and a manually entered name;
  - the creation and last-edit times;
  - the application version that generated it;
  - the results, with the labels and tags visible at the time;
  - the recorded dice;
  - the rules and modifiers that explain the result;
  - its favourite state.
- Changing tables or tags later does not change saved settlements. Only a
  deliberate edit or reroll updates a snapshot.
- **A favourited settlement cannot be deleted.** The user must unfavourite it
  first. This is enforced by the shared core, not only by the interfaces.
- **View** (Main Menu → View):
  - **Left pane (about 25%):** the saved settlements. Favourites have a gold
    star before their name.
  - **Right pane (about 75%):** the selected snapshot, with its
    generated-with version, and tags when Tag View is on.
  - **Keyboard:** Up/Down move through the list, Right moves to the actions,
    and Left returns to the list.
  - **Actions:** Edit, Favourite and Delete, or Edit and Unfavourite for a
    favourite. Delete is never offered for a favourite.
  - **Edit** reopens the snapshot unchanged. Saving creates an explicit
    revised snapshot, and Cancel or Esc discards the edit.

## 10. Generation

The pipeline is: Setting Profile → Region → Name → Environment →
Connections → Size → Origin → Purpose → Age → Economy → Authority
(→ Representation → Authority Size → Roles, sub-stages of Authority) →
Authority Figures → Security → Crime → Stability → Notable Locations. No
other stages are inserted.

### Notable Locations

- The count depends on the Size; Cities and Capitals first roll their number
  of districts, then a count per district. Each rolled location rolls its
  Type on a 1d20 (see [CONTENT.md](CONTENT.md)).
- The settlement sheet lists them numbered, the Bare Minimum Buildings Tavern
  first, each rolled location with its district.
- In Manual generation the District Count, each count and each rolled
  location's Type can be re-rolled or chosen. The guaranteed Tavern is fixed.
- Subtypes, details, NPCs, map placement and the Noble system's residences
  and estates are future work.

### Name (v0.1.1)

- **Data-driven.** Settlement names come from naming content (see
  [NAMING_SYSTEM.md](NAMING_SYSTEM.md)): Setting Profile → Region → naming
  scheme → naming method.
- **When and how.** The Name stage comes straight after Region, in every
  mode:
  - roll a name;
  - choose a naming method;
  - reroll the name alone;
  - or type a name, which is always possible.
- **Fallback.** Without naming data for the region, the stage is skipped and
  nothing is invented.
- **Tag View** shows the scheme routing and every roll.
- **God Mode** lifts method tag conditions without changing the settlement's
  tags.
- **Saving.** The save dialog offers the Name stage's name. The snapshot keeps
  the full name record, which is never regenerated. The content and the
  rules behind each stage are described in [CONTENT.md](CONTENT.md).

### Automatic

Automatic mode resolves every stage from beginning to end:

- it applies tags as it goes and uses the tag-selected table variants;
- it rolls physical dice and records every roll;
- it presents the completed settlement.

If required content is incomplete, generation stops with a human-readable
error and nothing is invented.

### Manual, Incremental

The stages are shown one at a time. At each stage the user sees the table and
its legal results, and can roll or directly choose a permitted result. Tags
are shown when Tag View is on. Moving forwards or backwards is safe.

### Manual, One-table

All stages are shown together. The user can inspect, roll, reroll or choose
any stage.

### Changing results

In both manual presentations, changing an earlier result re-checks the later
stages. A later stage whose rules changed is invalidated, and its previous
result stays visible. Stages that do not depend on the change are kept.

### Completed settlement

The completed settlement shows every stage result. Tags are shown only with
Tag View. It has three actions:

- **Re-roll** rerolls the entire settlement from scratch. The Setting Profile
  and any result that cannot be rolled are kept.
- **Save** asks for a name. The name must not be blank and is limited to 80
  characters. The snapshot is saved and the app returns to the Main Menu.
- **Delete** discards the unsaved settlement after a confirmation, then
  returns to the Main Menu.

Leaving generation with Esc or Back also asks first, once anything has been
generated. Automatic asks only for the Setting Profile. It stops with a
readable message wherever content is incomplete, and offers a choice there
so the user can continue.

### God Mode

God Mode makes every result of the current table selectable. It removes only
eligibility limits: the chosen result still emits its tags, and later rules
still apply.

### Reproducibility

The core stores enough to explain every result:

- the input tags;
- the variant;
- the dice expression and every die;
- secondary and follow-up rolls;
- the final result;
- the emitted tags.

Interfaces show these details only in Tag View or a debug view.

## 11. Status (v0.1.0 in progress)

Done:

- a Rust workspace with a shared core crate;
- Tauri GUI and Ratatui TUI shells, both calling the core and showing the
  version;
- in the core:
  - the data-driven content model;
  - the physical dice engine;
  - the generation engine and the session workflow for all modes;
  - validation;
  - default/override layering;
  - SQLite persistence of settings, customisations and saved settlements;
- the bundled v0.1.0 content (see CONTENT.md, including what is still
  incomplete).

Still to be specified and built:

- the content marked incomplete: non-Kurovia Environment;
- GUI: none outstanding beyond polish. The shell, Main Menu, Settings, Roadmap, Credits, both themes, and the Start
  generation flows (Automatic, Incremental, One-table, and the Completed
  Settlement with Re-roll, Save and Delete) and the saved-settlement View
  (favourites, edit, delete) are done. The Tag and Table editors are also done.
- the TUI is complete (feature parity with the GUI).
- signed auto-updates from GitHub Releases for the GUI:
  - the user is offered Update or Later and is never forced;
  - user data is backed up before installing;
  - upgrades are transactional (see RELEASING.md).
  - The signing key and the GitHub owner still have to be configured before
    the first public release.
- Settlement naming, released in v0.1.1:
  - the data-driven naming engine and the Name stage in every mode, in both
    interfaces;
  - Ordon naming content;
  - provisional sample packs for the other seven Kurovian regions: Halingo,
    Skgurr, Shinkira, Bratislor, Hjartheland, Nareth and Targoth.
  - Still to come: a generic scheme for other Setting Profiles.
- Released in v0.1.5:
  - Size distribution, Economy, Authority with Representation and Authority
    Size;
  - Authority Figures with species-specific names, and their Roles;
  - Security, Crime and Stability;
  - Notable Locations (count by Size and district, 1d20 Type) and the Bare
    Minimum Buildings setting.
- Windows release packaging: one per-user NSIS installer with the GUI
  (`settlement-generator.exe`) and the CLI (`settlement-generator-cli.exe`),
  built by `npm run release:build` (see RELEASING.md).
