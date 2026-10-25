# Flow Editor

Both interfaces (Settings › View Flows) edit flows without JSON. The GUI
shows a flow as a node graph; the terminal shows the same flow as an
indented tree. Every structural change is an edit the core applies, and all
wording (node captions, conditions, pseudocode, problems) comes from the
core, so the two interfaces behave the same.

| Core                             | Does                                                                                                    |
| -------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `framework::edit::apply`         | Applies one edit (add, insert, update, remove, move, detach, attach, duplicate, add/remove case, place) |
| `framework::edit::new_step`      | A new step of a kind, with a unique ID                                                                  |
| `framework::describe::Describer` | Conditions in words, node captions and the read-only pseudocode                                         |
| `Describer::context`             | What the pickers offer: earlier results, tables, tags, settings, objects, flows, step kinds             |
| `AppService::check_flow`         | Validation of a draft, with its pseudocode and node captions                                            |
| `AppService::save_flow`          | Saves a flow (refused with the reasons if it would make content invalid)                                |
| `AppService::*_flow_draft`       | Keeps, loads and discards an unsaved draft                                                              |
| `AppService::duplicate_flow`     | An unsaved custom copy of a flow                                                                        |

Flows are framework items: delete and restore go through
`delete_framework_item` and `reset_framework_item` like any other.

## Steps

The palette offers: **Roll Table**, **Run Subflow**, **If / Condition**,
**Create Object**, **Repeat N Times**, **Repeat For Each**, **Set Value**,
**Add Tag**, **Remove Tag**, **Apply Modifier**, **Read Setting** and
**Stop / End**. Name and built-in stages of the start flow are shown and can
be moved, labelled, conditioned or made inactive; their own settings stay on
their own screens. See [FRAMEWORK.md](FRAMEWORK.md#6-flows) for what each
step does.

## The node graph (GUI)

- A flow's steps run down a rail; a container's steps (a repeat's body, each
  case of a condition, Otherwise, an object's steps) hang off to its right,
  with the case written on the edge ("IF Authority is Military Authority").
- **+** marks every place a step can go. Click it to add a step there.
- **Drag** a step onto a **+** to move it (with everything inside it).
  Drag it onto empty space to disconnect it: it floats where it was dropped,
  dashed, and never runs until it is connected again by dragging it onto a
  **+**.
- Select a step to change it in the panel beside the graph. The panel also
  moves it up or down, duplicates, disconnects and deletes it.
- Pan by dragging the background; zoom with the wheel or the − / 100% / +
  buttons; **Fit to view** shows the whole flow. **Find a step** highlights
  matches; Enter jumps to the next one.
- Steps with problems are marked (● error, ○ warning). The **Problems** tab
  lists them; clicking one selects its step. The **Pseudocode** tab shows
  the flow in words; clicking a line selects its step.
- Keys: Ctrl+Z / Ctrl+Y undo and redo, Ctrl+D duplicates the selected step,
  Delete deletes it, Ctrl+S saves.

## The tree (terminal)

The same flow as rows: each step with its caption, each container's lists
under it ("Each time:", "IF … :", "OTHERWISE:"), and "+ add a step" at the
end of every list. Detached steps are listed under **Not connected**.

| Key           | Does                                                    |
| ------------- | ------------------------------------------------------- |
| Enter         | Opens a step's settings; on "+ add a step", adds one    |
| a / i         | Adds a step after / before the selected one             |
| x / Delete    | Deletes the step (asks first if it holds others)        |
| c             | Duplicates the step                                     |
| m, then Enter | Moves (or connects) the step to the place chosen        |
| J / K         | Moves the step down / up within its list                |
| d             | Disconnects the step                                    |
| w             | Edits the step's condition                              |
| / and n       | Finds a step, and the next match                        |
| u / U         | Undo / redo (also Ctrl+Z / Ctrl+Y)                      |
| Tab           | Steps → Pseudocode → Problems                           |
| s             | Saves                                                   |
| D / X / R     | Duplicates the flow / deletes it / restores its default |

## Conditions in plain words

Conditions read as sentences: **WHEN** [Authority] [is] [Military
Authority] **AND** [Representation] [is] [Group]. A test is one of:

- an earlier step's result: _is_ / _is not_ one or more of its results;
- an earlier step's number: _is exactly_, _between_, _at least_, _at most_;
- a tag: _is present_ / _is not present_;
- a setting: _is on_ / _is off_, or _is_ / _is not_ one of its options.

Tests are grouped as **All of these (AND)**, **Any of these (OR)** or
**None of these**, and groups can be nested. "Earlier results" lists the
flow's own steps and the pipeline's stages that store a result or number.
The terminal builds the same conditions (Earlier result is, Earlier number
is in a range, Setting option is, plus Has tag / All of / Any of / Not /
Setting is on) and shows the sentence above the tree.

## Pseudocode

Read-only, generated by the core from the flow:

```
Roll Authority.
Roll Representation.
IF Authority is Military Authority
AND Representation is Group
THEN run Authority Size.
Repeat Districts times:
    Create a District:
        …
```

## Safety

Validation runs on every change; a flow with errors cannot be saved:

- flows that run each other in a loop (an error, unless every Run Subflow
  in the loop has a maximum depth: deliberate recursion, then a warning);
- missing tables, results, tags, settings, object types and flows;
- conditions that can never hold, and steps that can never run (after a
  Stop, behind a case that always applies, or disconnected);
- loops that never run, and For Each over a type nothing creates.

## Drafts, undo and saving

Every change can be undone and redone. Unsaved changes are kept as a draft
in the core (debounced in the GUI, after every change in the terminal) and
offered again when the flow is reopened. Saving a bundled flow stores a
customisation; **Restore default** brings back the installed one. Saved
generations are snapshots and never change when a flow does.

## Patterns

Everything below is ordinary content, with no settlement-specific code:

- **N districts, N locations per district**: roll a count; Repeat (the
  count) › Create District › roll a count › Repeat (that count) › Create
  Location.
- **N authority figures**: Repeat (a count) › Create Figure.
- **One relationship per unique pair**: Repeat For Each Figure, _every
  unique pair_ › Roll Relationship.
- **Bare Minimum Buildings**: If / Condition _Bare Minimum Buildings is
  on_ › Run Subflow (the guaranteed buildings).

`crates/settlement-core/tests/flow_editor.rs` runs each of these.
