# Hjartheland - Expanded Naming Pack (v0.2.0 content draft)

## Purpose

One coherent, fully editable **Nordic and deep-winter-inspired** naming scheme for Hjartheland. Generated settlements follow cultural and historical *naming mechanisms*, not an unstructured heap of random syllables. The main orthographic flavour is fictionalised Norwegian / West Norse, with selective pan-Nordic influences.

**These are provisional names and spelling conventions, not assertions of authentic Norwegian, Icelandic or Swedish grammar.** No new Kurovia canon is established by installing this pack.

Hjartheland is characterised by tundra, glaciers, mountains and Forever Ice, while hospitality and refuge are important thematic contrasts. The magic Huld is established regional lore, but deliberately not injected into every name.

## Naming table and generation counts

| d100 | Method | Chance | Paths |
|---|---|---:|---:|
| 01-22 | Traditional Settlement Compound | 22% | 1,500 |
| 23-42 | Mountains, Ridges & Valleys | 20% | 1,200 |
| 43-63 | Snow, Glaciers & Winter | 21% | 960 |
| 64-73 | Rivers, Lakes & Fjords | 10% | 600 |
| 74-82 | Forest, Moor & Pasture | 9% | 600 |
| 83-93 | Shelter & Winter Routes | 11% | 600 |
| 94-97 | Historical Homestead | 4% | 480 |
| 98-100 | Ancient Inherited Name | 3% | 60 |
| **Total** | | **100%** | **6,000** |

There are **4,842 distinct case-insensitive spellings** and **1,158 extra generation paths that duplicate another generated spelling**. Collisions are recorded in `COLLISIONS.csv` and their generation paths in `COMPLETE_CATALOG.csv`. Duplicate spellings across methods are not automatically mistakes, but merit editorial review.

## What is included

- 16 independent physically-rollable JSON tables (`1d100`, `1d20` and `1d12`).
- The original stable table IDs `table.naming.hjartheland.method`, `.firstElement`, `.secondElement`, `.founder`, `.founderEnding`.
- All initial twelve traditional first elements, twelve historical suffixes, twelve founding-name IDs and six founder-ending choices carried forward from the first Hjartheland sample pack.
- One Hjartheland-only `naming-schemes.json` object, routed by `world.kurovia` AND `region.hjartheland`.
- `PREVIEW.csv` with 20 reproducible seeded samples per naming method.
- `COMPLETE_CATALOG.csv` listing every component pairing and every curated complete name.
- `COLLISIONS.csv` indicating duplicated output names.
- `METHOD_WEIGHTS.csv` recording method probabilities and combinatorial possibilities.
- `EXAMPLE_NAMES.md` with 16 selected name examples.
- `AUTHORING_PREVIEW.py`: offline no-dependency validator and preview tool for VS Code editing.
- `HISTORICAL_RESEARCH.md` with cited real-world reference links and editorial rationale.
- `EDITORIAL_NOTES.md` highlighting potential realism/compatibility improvements.
- `VALIDATION.md` with automated structural checks.
- `CLAUDE_INSTALL_PROMPT.md` to integrate safely into the active project.

## Important compatibility details

This authoring pack follows the existing **Bratislor schema_version 1** and the Halingo expanded naming-pack conventions. It is NOT verified against your running application because the actual core loader implementation was not available.

All methods use only the existing `join.style: compound`, including the one-component complete-name method. No new joining operator is needed. Before replacing content, check that the core supports a single-component naming method. If it does not, add that *generic* support once instead of a Hjartheland hardcode.

The archive contains a one-region `naming-schemes.json`. If your application has one global manifest for all regions, **merge only the Hjartheland scheme object**; never overwrite the other seven regions' scheme entries.

The method dice changes from an initial `1d10` to `1d100`, allowing editable percent weights. This is a legitimate content update; verify that no user override masks the new built-in defaults.

## Genitive / founder joining

The historical `table.naming.hjartheland.founder` has 40 *possessive stems*, not uninflected first names. Each stem's stable ID is the dictionary-form personal name (for example `eirik`), but its text is `Eiriks`. Direct joining with the `stad` ending produces **Eiriksstad**. This is a **simplified naming convention**, not a universal rule for Icelandic/Norwegian grammar. Keeping it in the data avoids new hardcoded linguistic logic in the shared engine.

## Editing in VS Code

1. Back up your current Hjartheland regional naming directory.
2. Replace the 16 component JSON files in that region's expected content folder, adopting your existing folder conventions.
3. Merge only the `naming.kurovia.hjartheland` object from the supplied regional `naming-schemes.json` into a global manifest, if appropriate.
4. Change component `name` values freely, while preserving stable `id` values to protect existing references and user overrides.
5. To change odds, update `01-method.json`: its `1d100` `range` entries must cover every face 1 through 100 exactly once.
6. To add an entry to a d100 table, redistribute its `range` faces. Never add an unassigned result without adding a compatible physical dice range.
7. Run `python AUTHORING_PREVIEW.py --samples 20` to validate the edited JSON and preview each method. Run `python AUTHORING_PREVIEW.py --samples 20 --export` to regenerate `PREVIEW.csv`, `COMPLETE_CATALOG.csv`, `COLLISIONS.csv` and `METHOD_WEIGHTS.csv`. The CLI/GUI shared Rust engine remains authoritative at runtime.
8. Restart/reload built-in content according to the current app; user overrides may take precedence over edited defaults.
9. Test generation in both GUI and CLI; preserve old saved settlement snapshots and manual name edits.

## Future context-aware weighting (not implemented)

- `environment.glacial`, `environment.foreverIce` could favour snow/glacier names.
- `environment.mountains` could favour mountain and winter-route names.
- `environment.tundra` could favour sparse pasture, winter, and old farmstead names.
- `connection.isolated` could favour shelter/route names.
- `origin.estate` could favour historical homesteads.
- `age.ancient` could favour inherited names.

These are merely proposed rule hooks: **do not silently alter method odds**. Physical dice equivalents and explicit tag-effect definitions are required before activating any of them.
