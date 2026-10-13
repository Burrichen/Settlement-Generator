# Ordon naming

Ordon settlement names come from the **Ordon Quality Pass v2** naming pack: five
naming methods, a hand-reviewed vocabulary, and individually approved final
names wherever free joining produced poor results (no more _Oswaldsworth_,
_Foundrywell_ or _Market Market_).

Everything here is data. No Ordon name, weight or rule is written in code.
For the general naming engine, see [NAMING_SYSTEM.md](NAMING_SYSTEM.md).

## How a name is chosen

1. **Scheme.** Once the Setting Profile and Region are known, the scheme
   `naming.kurovia.ordon` applies when the tags `world.kurovia` **and**
   `region.ordon` are both active. Routing uses tags, never display names, so
   renaming the region changes nothing.
2. **Method.** `table.naming.ordon.method` is rolled on **1d100**. Its ranges
   are the weights:

   | d100   | Method (result ID)                            | Chance |
   | ------ | --------------------------------------------- | -----: |
   | 01–42  | Old English & Landscape (`geographic`)        |    42% |
   | 43–62  | Founders, Manors & Estates (`founder`)        |    20% |
   | 63–80  | Victorian Works & Industry (`industrial`)     |    18% |
   | 81–92  | Railways, Canals & Market Roads (`route`)     |    12% |
   | 93–100 | Parish, Civic & Inherited Names (`inherited`) |     8% |

   Choosing a method by hand skips this roll; the trace shows it was chosen.

3. **Parts.** The method's component tables are rolled in order, with
   physical dice.
4. **Join.** The parts become one name by the method's join rule, after its
   spelling exceptions are checked. Nothing is ever rerolled to improve a
   name.

## Files

All in `content/kurovia/naming/ordon/`, one table per file:

| File                      | Table ID (`table.naming.ordon.…`) | Dice  | Contents                                                |
| ------------------------- | --------------------------------- | ----- | ------------------------------------------------------- |
| `01-method.json`          | `method`                          | 1d100 | The five methods and their weights                      |
| `02-first-element.json`   | `firstElement`                    | 1d100 | 50 landscape stems, 2 faces each                        |
| `03-second-element.json`  | `secondElement`                   | 1d20  | 20 endings, 1 face each                                 |
| `04-founder.json`         | `founder`                         | 1d100 | 100 founders, each with approved names `a` and `b`      |
| `05-founder-ending.json`  | `founderEnding`                   | 1d4   | The approved form: A on 1–2, B on 3–4                   |
| `06-industry-first.json`  | `industryFirst`                   | 1d100 | 40 trades or works, each with approved names `a`–`d`    |
| `07-industry-second.json` | `industrySecond`                  | 1d4   | The approved form: A 1, B 2, C 3, D 4                   |
| `08-route-first.json`     | `routeFirst`                      | 1d100 | 30 transport and civic sources, approved names `a`–`d`  |
| `09-route-second.json`    | `routeSecond`                     | 1d4   | The approved form: A 1, B 2, C 3, D 4                   |
| `10-inherited-name.json`  | `inheritedName`                   | 1d100 | 50 complete names, 2 faces each                         |
| `naming-schemes.json`     | —                                 | —     | The Ordon scheme only: methods, joins and 17 exceptions |

Each region has its own `naming-schemes.json`; together they form the
registry. Editing Ordon's file cannot affect another region.

The authoring tools that shipped with the pack (preview script, catalogues,
editorial notes, archived legacy suffix lists) are in
[`authoring/naming/ordon/`](../authoring/naming/ordon/). That folder is
**not** bundled into the app.

## The five methods

| Method       | Parts                  | Join               | Paths | Example                   |
| ------------ | ---------------------- | ------------------ | ----: | ------------------------- |
| `geographic` | first element + ending | `compound`         | 1,000 | Alder + wick → Alderwick  |
| `founder`    | founder + form (A/B)   | `curated_variant`  |   200 | Oswald + B → Oswaldwick   |
| `industrial` | trade + form (A–D)     | `curated_variant`  |   160 | Foundry + A → Foulden     |
| `route`      | source + form (A–D)    | `curated_variant`  |   120 | Market + A → Market Cross |
| `inherited`  | the complete name      | `compound`, 1 part |    50 | Netherleigh               |

**1,530 paths** in all, giving **1,497 distinct spellings**: 33 spellings
can be reached by more than one path (e.g. Alderton is both Alder + ton and
Aldric's form A). Duplicates are allowed and are not errors.

- **Geographic** joins the two parts directly, with the first letter capital.
  The 17 spelling exceptions (e.g. Ash + ham → Asham, Hill + ley → Hildley,
  Mill + ley → Milnley, Fair + well → Fairbourne) give exact spellings for
  particular pairs.
- **Founder, Industry and Transport** never build a name. The first part's
  result holds its approved names (`approved_names`); the second part's
  result ID (`a`, `b`, …) picks one, used exactly as written: spaces,
  apostrophes and capitals included. No suffix or possessive is ever added:
  _Godwinsbury_ has its possessive because it is written that way.
- **Inherited** names are used whole, exactly as written (St. Cuthred,
  Regent's Vale).

### `curated_variant`

A generic join style, usable by any scheme:

```json
"join": {
  "style": "curated_variant",
  "source_field": "approved_names",
  "variant_from_result_id": 1,
  "on_missing": "error"
}
```

- `variant_from_result_id` is the (zero-based) component whose result **ID**
  selects the name: `1` is the second part. `source_from_result_id` (default
  `0`) is the component whose result holds the names.
- `source_field` names the result field holding them. Results store approved
  names in `approved_names`, the only field supported.
- `on_missing: "error"`: a missing or empty name is a validation error. The
  engine never falls back to joining the parts and never rerolls.

Validation requires every source result to have a non-empty name for
**every** result of the variant table, and no names for IDs that are not
variants. Problems are reported on the source result, e.g.
`table `table.naming.ordon.founder`› result`oswald`: no approved name for
variant "B"`.

## Physical dice

Every name can be reproduced at the table:

| Method     | Rolls                                 | Example                                                  |
| ---------- | ------------------------------------- | -------------------------------------------------------- |
| Geographic | 1d100 method, 1d100 stem, 1d20 ending | 14 → Geographic; 24 → Hart; 16 → gate: **Hartgate**      |
| Geographic | (exception)                           | 30 → Geographic; 3 → Ash; 2 → ham: **Asham**, not Ashham |
| Founder    | 1d100 method, 1d100 founder, 1d4 form | 50 → Founder; 28 → Oswald; 3 → B: **Oswaldwick**         |
| Industry   | 1d100 method, 1d100 trade, 1d4 form   | 63 → Industry; 41 → Foundry; 1 → A: **Foulden**          |
| Transport  | 1d100 method, 1d100 source, 1d4 form  | 81 → Transport; 53 → Market; 1 → A: **Market Cross**     |
| Inherited  | 1d100 method, 1d100 name              | 95 → Inherited; 1 → **Netherleigh**                      |

Tag View shows each roll, its table, the stable result ID and how the name
was assembled (the join, any exception, or the approved name read).

## Editing in the app

**Settings → View Tables** (GUI) or **View Tables** (terminal). Every change
is stored as a user customisation over the bundled default; updates never
overwrite it, and **Reset** restores the default.

- **Approved names:** open the source table (e.g. _Ordon Manorial:
  Historical Person or Landholder_). Each result shows one field per variant
  (Variant A, Variant B …), each edited on its own. Emptying one is refused
  with the table, result and variant named.
- **Joins and exceptions:** open `Ordon Naming Method`. Its **Naming
  methods** section lists the five methods: the join style and its settings,
  and the spelling exceptions (pick each part's result, type the exact
  name, add a note). Each method is saved and reset separately from the
  table.
- **Names, dice, weights and entries** are edited like any other table.

In the terminal, the same edits are in the table editor: a result's page has
an **APPROVED NAMES** section, and the method table's page lists **NAMING
METHODS**, each with its own page.

## Editing in VS Code

Edit the JSON in `content/kurovia/naming/ordon/`, then check it (below).

1. **A geographic first element:** in `02-first-element.json`, change its
   `"name"`; keep its `"id"`.
2. **A geographic ending:** the same, in `03-second-element.json`.
3. **A founder's name A or B:** in `04-founder.json`, edit
   `"approved_names": { "a": "…", "b": "…" }` for that founder.
4. **An industrial name:** `06-industry-first.json`, `approved_names` `a`–`d`.
5. **A transport name:** `08-route-first.json`, `approved_names` `a`–`d`.
   Spaces are kept exactly ("Market Cross").
6. **An inherited name:** in `10-inherited-name.json`, edit a `"name"`, or
   add `{ "id": "new-id", "name": "Newname", "range": [n, m] }` and take its
   faces from a neighbour.
7. **Method probabilities:** edit the `range`s in `01-method.json`. Faces are
   weights: 42 faces on 1d100 is 42%.
8. **A spelling exception:** in `naming-schemes.json`, add to the method's
   `exceptions`:
   `{ "results": ["oak", "ton"], "name": "Ockton", "note": "Why" }`.
   `results` lists component result **IDs** in order, so renaming a component
   never breaks the exception. The pack's original by-name form
   (`{ "components": ["Oak", "ton"], "output": "Ockton" }`) is still read, but
   breaks if a display name changes (validation then reports it).
9. **Another approved variant** (e.g. a form E): add a result to the variant
   table (`07-industry-second.json`) and change its dice (e.g. to 1d6 with E
   on 5–6), then add an `"e"` approved name to **every** source result. Until
   each has one, validation lists every source that is missing it.
10. **Validate:** `cargo test -p settlement-core`. It reports gaps, overlaps,
    faces outside the dice, unknown IDs and missing approved names, each with
    its location. `tests/ordon_naming.rs` checks the pack's counts, weights
    and examples; update it when you change them on purpose.
11. **Preview names:**
    `python3 authoring/naming/ordon/AUTHORING_PREVIEW.py --samples 20 --export`
    validates the tables and rewrites `COMPLETE_CATALOG.csv`,
    `COLLISIONS.csv` and `PREVIEW.csv` beside the script. The test
    `the_engine_matches_the_authoring_catalogue` checks the app produces
    exactly the catalogue's names, so re-run the script after editing.
12. **Reset user overrides:** in the app, **Reset to default** on a table or
    method, or **Restore all defaults** in View Tables.
13. **When an override masks your edit:** a table or method you changed in
    the app keeps its customised fields after you edit the JSON. View Tables
    then shows a warning: _the bundled default changed after the user
    customised it_. Reset that item to see the bundled version.

### Stable IDs

Display names (`"name"`) can be changed freely. IDs (`"id"`) are what
exceptions, approved-name keys, saved settlements and user customisations
refer to. Changing an ID is a structural migration, not a text edit: update
every reference to it, and expect customisations of the old ID to be
ignored.

## Saved settlements

A generated name is saved with its scheme, method, every roll (dice, totals,
result IDs and names) and how it was assembled: the join style, plus any
exception or approved name used. It is never regenerated: renaming a
founder, editing an approved name, changing weights or exceptions, updating
the pack or changing theme leaves saved names exactly as they were. A typed
name is saved as typed, with no rolls.

Names saved before this pack (e.g. _Oswaldsworth_ from the old founder join)
keep their saved text.

## Upgrading from the earlier Ordon pack

The pack replaced the earlier Ordon files in place, with the same table and
scheme IDs. Customisations of the old tables are kept: those of results that
still exist (most founders and stems) still apply, and those of results that
no longer exist (the old founder endings `ton`, `bury`, `worth`…) are ignored
with a warning. If a customisation makes the new content invalid (for example,
a custom result added to the old founder-ending table, which now needs
approved names), View Tables reports it and generation stops until it is fixed
or reset. Bundled content is never silently reloaded over it.
