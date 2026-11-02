# Table Editor

Both interfaces (Settings › View Tables) edit tables without any JSON or
formulas. The decisions (which dice fit, what is uncovered, what a total
gives, what depends on what) are made by the core, so the GUI and the
terminal interface behave the same:

| Core                          | Does                                                                       |
| ----------------------------- | -------------------------------------------------------------------------- |
| `dice_assist::advise`         | Suggests dice for results with given relative chances (the Dice Assistant) |
| `dice_assist::fit`            | Lays results out on dice the user picked                                   |
| `table_editor::apply_fit`     | Applies a layout to a table (dice, faces, the "Roll again" entry)          |
| `table_editor::table_usages`  | Everything that depends on a table                                         |
| `table_editor::result_usages` | Everything that depends on a result                                        |
| `AppService::preview_roll`    | Rolls a draft once with its own dice, die by die                           |
| `AppService::copy_table`      | A new custom table copied from an existing one                             |
| `AppService::*_table_draft`   | Keeps, loads and discards an unsaved draft                                 |

## The simple view

A table first shows only:

- **Table name**;
- **Chance mode**: **Equal** (every result equally likely) or **Custom**
  (each result has a relative chance: 2 is twice as likely as 1);
- **Dice**;
- **Results**, as spreadsheet rows: the result, its dice range, its exact
  chance, its relative chance (Custom), and an optional shared result.

In Equal and Custom mode, adding, removing, disabling, reordering a row, or
changing a relative chance or the dice lays the faces out again on the
current dice. Faces can still be typed by hand. Tables rolled with a sum of
dice (e.g. 2d6) keep their faces as typed, since faces of a sum are not
equally likely.

Everything else (description, how results are found, active, eligibility,
variants, follow-up rolls, every face of the dice, what uses the table) is
under **Advanced**, collapsed. Each result's own advanced options (emitted
tags, eligibility, tables rolled next, uniqueness and fallback, the number
it stands for, display variants, making a shared result from it) are
collapsed under its **More** button (GUI) or on its own page (terminal).

## Dice Assistant

The assistant reads the results' relative chances and suggests physical
dice, best first:

1. **Exact on one die**: the smallest standard die (d4, d6, d8, d10, d12,
   d20, d100) the chances share out on. 5 equal results → **1d10, 2 faces
   each**.
2. **Exact on a sum of dice** (unequal chances only): a bell curve such as
   2d6 whose totals match the chances exactly.
3. **Roll again**: a die with a few spare faces that are rolled again.
   Still exact: 7 equal results → **1d8, roll again on an 8**. The spare
   faces belong to a disabled **Roll again** result, which the engine
   rerolls, so the table stays a real tabletop table.
4. **Approximate**: the closest layout on 1d20 or 1d100, with the largest
   difference from the wanted chances stated.

When no clean die fits, it says why in plain English (for example "7 does
not divide evenly into 4, 6, 8, 10, 12 or 100 faces") and, for equal
chances, how many results to add or remove for one that does (7 → remove 1
for 1d6, or add 1 for 1d8). For custom chances it says which total the
chances could add up to instead.

It also lists, in plain English, uncovered faces, faces claimed by more
than one result, and ranges the dice cannot roll, beside the exact
percentage of every result.

Nothing ever falls back to software picking a result: every suggestion is
a layout of faces on real dice.

## Quality of life

| Feature              | GUI                                    | Terminal                                    |
| -------------------- | -------------------------------------- | ------------------------------------------- |
| Undo / redo          | Undo and Redo buttons, Ctrl+Z / Ctrl+Y | `u` / `U`, Ctrl+Z / Ctrl+Y                  |
| Keep a draft         | Automatic, shortly after each change   | Automatic, after each change                |
| Restore a kept draft | "Restore them" / "Discard them"        | Same, at the top of the table               |
| Create from existing | "Copy to a new table"                  | "Copy to a new table", or `c` in the list   |
| Reset to default     | "Reset to default" (customised tables) | Same                                        |
| Safe deletion        | Refused while used, naming each use    | Same                                        |
| Search / filter      | Table list filter; "Find a result"     | `/` in the list                             |
| Validation summary   | Under the name                         | At the top                                  |
| Preview roll         | "Preview roll"                         | "Preview roll"                              |
| Test table           | "What did the dice show?"              | "Test a dice total…"                        |
| Reorder / duplicate  | ↑ ↓ and ⧉ on each row                  | Move up / Move down / Duplicate on a result |

- **Drafts** are kept in the user database (the `settings` table, under
  `draft.table.<id>`), so they survive closing the app. Saving the table,
  discarding changes, resetting it or deleting it removes its draft.
- **Preview roll** rolls the draft once with its own dice, die by die, as
  generation would with no tags active; nothing is saved.
- **Test table** shows which result a total gives (or that it is
  uncovered, claimed twice, or impossible).
- **Safe deletion**: a table is not deleted while a pipeline step, flow,
  tag effect, scheme, built-in step or another table's result uses it; a
  result is not removed while a follow-up roll, fallback, variant,
  restriction, naming scheme, built-in step or flow step names it. The
  editor lists each use. A result can be disabled instead.

## Customisations

Edits are stored as customisations, as before (see
[DATA_MODEL.md](DATA_MODEL.md) §6). New fields:

- a result's `then_roll` (tables rolled next) and `shared` link are
  customisable;
- reordering a built-in table's results is kept as the patch's `order`.
  Results added by a later update keep their place after the listed ones.

Customisations saved by earlier versions load unchanged.
