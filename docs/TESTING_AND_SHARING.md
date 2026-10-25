# Testing, Tracing, Sharing and Migration

The tools that finish the no-code framework: test any table or flow,
explain every generated value, share content as files, see (and open)
whatever depends on an item before deleting it, and keep everything from
the released app. All of it lives in the core, so the GUI and the
terminal behave the same.

| Core                                                                    | Does                                                                    |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `simulate::simulate_table`                                              | Rolls a table up to 10,000 times; expected vs actual for every result   |
| `simulate::simulate_flow`                                               | Runs a flow (or whole generations, for the pipeline) up to 10,000 times |
| `simulate::preview_flow`                                                | One run of a flow, every roll explained (Roll once, Step through)       |
| `Generator::expected_chances`                                           | Each result's exact chance from the dice and the table's rules          |
| `trace::trace_roll`                                                     | One roll explained line by line                                         |
| `transfer::{export, plan_import, import}`                               | Content files: export, check, import with ID collisions handled         |
| `dependencies::{tag,setting,flow}_usages`, `table_editor::table_usages` | What depends on an item                                                 |

## Test and preview

Every table editor has a **Test this table** section, and the flow editor
a **Test** tab (`t` in the terminal):

- **Roll once**: one roll (or one run of the flow) with every roll
  explained.
- **Step through**: the same, one line at a time (**Next step**).
- **Run 100 / 1,000 / 10,000**:
  - a table: each result's **expected** chance (from the dice and the
    table's rules, Shift and Cap included; follow-up rolls are not) next
    to how often it **actually** came up, with a bar; results that are
    **never possible** now; how many rolls were made again on average;
  - a flow: what each step gave, how often each case of every condition
    was taken, and **branches that never ran** (with the conditions
    validation proves impossible);
  - both: **modifiers used** (with their tag or step), runs that stopped
    with an **error**, and validation problems.

Tests use software dice and save nothing; generation itself still uses
physical dice. A table can be tested with tags active (Options… in the
GUI). The pipeline flow is tested as whole generations, built-in stages
included; tables that are chosen get a random choice. Any other flow runs
on its own, without the stages before it. A draft with errors cannot be
tested. 10,000 whole generations take a few seconds, so the GUI runs
tests in the background.

## Generation trace (Roll details)

Settings › **Roll details (advanced)** shows, under each stage, how every
value was reached:

```text
Crime
Base roll: 10 (1d12: 10)
Security modifier: -1
Economy modifier: +2
Final roll: 11
Rolled result: High
Shift +1 (Economy): High → Very High
Result: Very High
```

Each roll lists: rolls made again (and why), the base roll and its dice,
every modifier by its source (tag or step), the final total (and when it
was read as the table's end), the scale path of Shift and Cap, follow-up
rolls, choices, redirects, routes and variants. The trace is read from the
roll's own record, so it explains saved generations too, and it never
changes what is generated.

## Import and export

Settings › **Import / Export**, and an **Export** button in each table,
tag, setting and flow editor.

- **Export everything I made or changed** (a backup of your work), **a
  complete content pack** (everything), or **chosen items**.
- With "include what they need", custom items an export refers to come
  along; unchanged defaults do not (every installation has them; the file
  names them by ID).
- Files are JSON (`"format": "settlement-generator-content"`, `"version":
1`), saved in `Documents/Settlement Generator`.

Importing shows what will happen to each item before anything changes, and
checks every reference: a file that would leave anything pointing at
something missing is refused, and nothing is imported. When an item's ID is
already used:

| Choice                  | Effect                                                                     |
| ----------------------- | -------------------------------------------------------------------------- |
| **Keep both** (default) | It comes in as a new copy with a new ID; references inside the file follow |
| **Replace mine**        | It replaces yours (a default becomes customised; restore it any time)      |
| **Keep mine**           | It is not imported; references inside the file use yours                   |

An item with a non-user ID that is not a default here is always imported
under a new user ID. Files from a newer version are refused with a
request to update.

## Dependency safety

- Tags, settings, tables and flows list what depends on them (**Used by** /
  **Dependencies**), and each entry opens the dependent table, tag or flow
  directly (a link in the GUI, Enter in the terminal).
- **Delete** first shows those uses; while any remain nothing is deleted
  (the core refuses too, naming them).
- Every other edit, import and restore is checked the same way: anything
  that would break a reference is refused with the reasons. References
  never break silently.

## Migration from the released app

The released app's data opens unchanged:

- **Tables and tags** customised in the released app, and custom tags, are
  kept (user overrides stay separate from the bundled content, which is
  never changed).
- **Settings** are kept; Bare Minimum Buildings becomes the content setting
  `setting.bareMinimumBuildings` with the same value.
- **Saved settlements and favourites** read exactly as before, including
  Authority Figures, roles and Notable Locations, and reopen for editing
  unchanged.
- The generation pipeline is now the flow `flow.settlement`; Authority and
  Notable Locations are its built-in stages. **Generation results** are the
  same for the same dice.

`crates/settlement-core/tests/migration.rs` opens a database written by the
released app (`tests/fixtures/released-0.1`) and checks each of these;
`tests/golden_outcomes.rs` pins 480 seeded generations from before the
framework.

## Simple by default

Editors open with the essentials; testing results, roll details, export
choices and import options appear only when asked for. No step needs JSON.
