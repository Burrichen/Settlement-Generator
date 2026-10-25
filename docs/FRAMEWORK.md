# Generation Framework

> **Status:** implemented in `settlement-core` (`framework/`, plus the
> framework parts of `content.rs`, `engine.rs`, `session.rs`, `layers.rs` and
> `validation.rs`). The Settlement Generator's own pipeline is defined with it
> (`content/00-pipeline.json`).

The framework is a set of generic building blocks: tables, results, tags,
conditions, modifiers, scales, settings, objects and flows. A generator is
made by combining them as **data**, without code. Nothing in the framework
names a settlement concept; the settlement content is one generator built
from it.

Every rule below still rolls **physical dice**: a flow never picks at random
by itself. Each roll goes through the same table engine as every stage, so
dice, redirects, variants, eligibility, God Mode, follow-up rolls and every
recorded die work identically everywhere.

The format details of tables and tags are in [DATA_MODEL.md](DATA_MODEL.md).
Flows are edited visually as described in [FLOW_EDITOR.md](FLOW_EDITOR.md);
tables in [TABLE_EDITOR.md](TABLE_EDITOR.md); tags, settings and modifiers in
[RULES_EDITORS.md](RULES_EDITORS.md); testing, roll details, import/export
and migration in [TESTING_AND_SHARING.md](TESTING_AND_SHARING.md).

## Contents

1. [Building blocks](#1-building-blocks)
2. [Shared results](#2-shared-results)
3. [Ordered scales and modifiers](#3-ordered-scales-and-modifiers)
4. [Conditions](#4-conditions)
5. [Settings](#5-settings)
6. [Flows](#6-flows)
7. [Objects](#7-objects)
8. [Built-in steps](#8-built-in-steps)
9. [Planned and inactive rules](#9-planned-and-inactive-rules)
10. [Validation](#10-validation)
11. [Defaults, customisations and saved generations](#11-defaults-customisations-and-saved-generations)
12. [Limits and recursion protection](#12-limits-and-recursion-protection)

## 1. Building blocks

| Block             | What it is                                                                   | Where                        |
| ----------------- | ---------------------------------------------------------------------------- | ---------------------------- |
| **Table**         | Results laid out on dice faces, rolled with physical dice (or chosen)        | `tables`                     |
| **Result**        | One outcome of a table: name, tags, value, eligibility                       | a table's `results`          |
| **Shared result** | A canonical result defined once and listed by several tables                 | `shared_results`             |
| **Tag**           | A label a result emits; carries effects that change later tables             | `tags`                       |
| **Condition**     | A test over tags, settings and earlier results, combined with AND / OR / NOT | anywhere a `when` appears    |
| **Modifier**      | Shift, Plus, Minus, Cap at Highest, Cap at Lowest                            | tag effects and flow steps   |
| **Scale**         | An ordered list of results that Shift and Cap move along                     | `scales`, or `ordered` table |
| **Setting**       | A choice the user makes (toggle or option list) that conditions can test     | `settings`                   |
| **Object type**   | A kind of generated thing (e.g. a person) with its own fields and tags       | `objects`                    |
| **Flow**          | An ordered list of steps: rolls, branches, repeats, subflows, objects        | `flows`                      |
| **Start flow**    | The flow a generation runs; each top-level step is one stage                 | `start`                      |

All of these have **stable IDs** separate from their display names. IDs use
`^[A-Za-z0-9][A-Za-z0-9_.-]*$`, and items the user creates use the reserved
`user.` namespace, so an update can never collide with them.

A table can be used by any number of flows, and a result (through a shared
result) by any number of tables.

## 2. Shared results

A shared result is defined once:

```json
{ "id": "environment.woodland", "name": "Woodland", "tags": ["env.woodland"] }
```

and listed by any table:

```json
{ "id": "woodland", "shared": "environment.woodland", "range": [9, 11] }
```

- The table entry keeps its **own ID** (unique within the table). Saved
  generations and customisations refer to it, so linking a result to a
  shared one never changes a saved record.
- The shared result's `name`, `description`, `tags`, `eligibility`, `value`,
  `label_variants` and `generator` are used for every field the entry leaves
  empty. Faces (`range`), `enabled`, `unique` and `fallback` always belong to
  the table.
- A table may give a field its own value (e.g. a different name) and keeps
  it.
- Customising a shared result changes every table entry that still uses its
  value; entries with their own value keep theirs.

The bundled content shares the Kurovian environments
(`content/03-environment/shared-results.json`) and the role titles that more
than one role table offers (`content/12-authority-roles/shared-results.json`).

## 3. Ordered scales and modifiers

### Scales

A **scale** is an order of results, lowest first. A table is on a scale when:

- it is `ordered` (its results, in listed order, are its scale); or
- it names a `scale`:

```json
{ "id": "scale.size", "name": "Size", "steps": ["size.small", "size.large"] }
```

A table result is on a named scale when its ID, or the ID of the shared
result it lists, is one of the steps. One scale can therefore serve several
tables.

### Modifiers

| Modifier           | JSON in a flow step | Tag effect `kind`     | What it does                                                              |
| ------------------ | ------------------- | --------------------- | ------------------------------------------------------------------------- |
| **Shift +X / −X**  | `{ "shift": 2 }`    | `shift` with `steps`  | Moves a rolled result X steps up (or down) the scale                      |
| **Plus X**         | `{ "plus": 2 }`     | `plus` with `amount`  | Adds X to the rolled total before it is read                              |
| **Minus X**        | `{ "minus": 1 }`    | `minus` with `amount` | Takes X from the rolled total before it is read                           |
| **Cap at Highest** | `"cap_at_highest"`  | `cap_at_highest`      | The rolled result becomes the highest result on the scale that can be had |
| **Cap at Lowest**  | `"cap_at_lowest"`   | `cap_at_lowest`       | The rolled result becomes the lowest result on the scale that can be had  |

Shift +1, Shift +2, Shift −1 and Shift −2 are offered as one-click presets
in both tag editors; any other whole number is allowed (up to ±100).

**Shift and Plus/Minus are different.** Plus and Minus change the **number
rolled**: on 1d6 with Plus 2, a roll of 3 is read as face 5, and a total past
either end of the table is read as that end. Shift moves along the **results**
after the roll is read: Shift +1 on "Stable" gives the next result up the
scale, whatever its faces.

How they combine, for one roll:

1. Plus and Minus are added to the total (as before, a total past either end
   is read as that end).
2. The face is read and rerolled as usual if its result cannot be had.
3. All Shifts are added up and the result moves that many steps. A step
   passes over results that cannot be had now (disabled, restricted,
   ineligible, or not on the current layout). The move stops at either end
   of the scale.
4. A Cap then gives the highest (or lowest) result that can be had. If both
   kinds of Cap apply, the last one wins.
5. Follow-up rolls apply to the result the modifiers gave.

Modifiers only change **rolled** results. A result chosen by the user stays
as chosen. The record keeps the roll, the result it read (`adjusted_from`)
and each modifier that applied (`modifiers`), so a saved generation explains
itself.

A tag effect modifier applies whenever its table is rolled while the tag is
active. A flow step can carry its own, optionally conditional, modifiers:

```json
{
  "id": "grade",
  "kind": "roll",
  "table": "table.grade",
  "modifiers": [{ "modifier": { "shift": 2 }, "when": { "has_tag": "tag.blessed" } }]
}
```

## 4. Conditions

A condition is a JSON object with one key:

| Condition                                                     | True when                                                            |
| ------------------------------------------------------------- | -------------------------------------------------------------------- |
| `{ "has_tag": "tag.x" }`                                      | the tag is active                                                    |
| `{ "all": [ … ] }`                                            | every condition inside holds (AND)                                   |
| `{ "any": [ … ] }`                                            | at least one holds (OR)                                              |
| `{ "not": … }`                                                | the condition inside does not hold                                   |
| `{ "setting_on": "setting.x" }`                               | the toggle setting is on                                             |
| `{ "setting_is": { "setting": "setting.x", "value": "a" } }`  | the dropdown setting has that option (or the text setting that text) |
| `{ "setting_between": { "setting": "setting.n", "min": 2 } }` | the number setting is in range (`min`, `max` optional)               |
| `{ "setting_has": { "setting": "setting.m", "value": "a" } }` | the multi-select setting has that option ticked                      |
| `{ "result_is": { "step": "size", "results": ["city"] } }`    | **flows only:** an earlier step gave one of these results            |
| `{ "value_in": { "step": "size", "min": 2, "max": 5 } }`      | **flows only:** the number an earlier step stored is in range        |

`result_is` and `value_in` read what an earlier step **stored** (see §6).
They can read any earlier step of the same flow, and any stage of the start
flow before it. Outside flows (in table eligibility, for example) only tag
and setting tests are allowed; validation reports `condition_needs_flow`
otherwise.

Tags affect both tables (through effects and eligibility) and flow decisions
(through conditions on steps and branches).

## 5. Settings

A content setting is a choice the user makes in Settings, which conditions
can test. There are five kinds:

```json
{ "id": "setting.ruins", "name": "Include ruins", "kind": "toggle", "default": false }
{ "id": "setting.districts", "name": "Districts", "kind": "number", "default": 3, "min": 1, "max": 12 }
{ "id": "setting.tone", "name": "Tone", "kind": "choice", "default": "calm",
  "options": [ { "id": "calm", "name": "Calm" }, { "id": "grim", "name": "Grim" } ] }
{ "id": "setting.biomes", "name": "Biomes", "kind": "multi_select", "default": ["forest"],
  "options": [ { "id": "coast", "name": "Coast" }, { "id": "forest", "name": "Forest" } ] }
{ "id": "setting.motto", "name": "Motto", "kind": "text", "default": "" }
```

| Kind           | Value                   | Tested with                  | Read Setting stores     |
| -------------- | ----------------------- | ---------------------------- | ----------------------- |
| `toggle`       | `true` / `false`        | `setting_on` (`not` for off) | 1 or 0                  |
| `number`       | a whole number          | `setting_between`            | the number              |
| `choice`       | an option ID            | `setting_is`                 | the option, as a result |
| `multi_select` | a list of option IDs    | `setting_has`                | how many are ticked     |
| `text`         | text (≤ 200 characters) | `setting_is`                 | the text, as a result   |

- Both interfaces list content settings in Settings, under Generation, and
  save changes immediately. Settings › View Generation Settings creates and
  edits the definitions (see [RULES_EDITORS.md](RULES_EDITORS.md)).
- `"status": "planned"` documents a setting without offering it:
  generation uses its default.
- Option IDs are fixed when an option is added, so renaming an option (or
  the setting) never breaks a condition.
- Validation: a test must suit the setting's kind (`setting_kind_mismatch`),
  dropdowns and multi-selects need options (`setting_without_options`), a
  default must be a value the setting can take (`invalid_setting_default`),
  and a number's lowest value cannot be above its highest
  (`invalid_setting_range`).
- The values are stored in the user database (`settings` key
  `content_settings`). A setting the user never changed takes its default.
- God Mode (`app.godMode`) is the application's own setting and can be
  tested too. Content settings cannot use the `app.` prefix.
- The settings a generation ran with are recorded in its log.

**Bare Minimum Buildings** is an ordinary setting declared by the bundled
content (`setting.bareMinimumBuildings`, in `content/00-pipeline.json`);
the Notable Locations stage tests it with `setting_on`. Its former ID
`app.bareMinimumBuildings` still names it. A choice made before it became a
content setting is kept, and changes are mirrored to the old application
setting. A generation records its value in its options, as saved
generations always have, so old and new saves read the same way.

## 6. Flows

A flow is a named list of steps:

```json
{ "id": "flow.market", "name": "Market", "steps": [ … ] }
```

Every step has an `id` (unique within its flow, nested steps included) and a
`kind`. Optional on every step: `name` (the label shown), `description`,
`when` (a condition; when false the step is skipped), `active` (`false`
skips it), `status` (`planned`, see §9), `note`.

| `kind`           | Fields                                          | Does                                                                                                 |
| ---------------- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `roll`           | `table`, `field`†, `modifiers`†                 | Rolls (or uses the chosen result of) the table with physical dice. Stores the result and its `value` |
| `secondary_roll` | `label`, `dice`                                 | Rolls extra dice and stores the total                                                                |
| `add_tag`        | `tag`                                           | Emits a tag, as a result would (its effects run)                                                     |
| `set_result`     | `table`, `result`, `field`†                     | Gives a fixed result without rolling; stores it                                                      |
| `set_value`      | `field`, `value`                                | Gives the current object's field a fixed number; stores it                                           |
| `branch`         | `cases: [{ when, steps, name† }]`, `otherwise`† | Conditional routing: runs the steps of the first case whose condition holds, else `otherwise`        |
| `repeat`         | `count`, `steps`                                | Runs the steps a number of times (see below)                                                         |
| `run_flow`       | `flow`, `max_depth`†                            | Runs another flow here (a subflow). Flows are reusable. `max_depth` allows deliberate recursion      |
| `for_each`       | `object`, `pairs`†, `steps`†                    | Runs the steps once per object of that type made so far (or once per unique pair, with `pairs`)      |
| `remove_tag`     | `tag`                                           | Makes a tag inactive from here on (effects it already applied stay)                                  |
| `apply_modifier` | `table`, `modifier`                             | From here on in the generation, every roll of the table gets the modifier                            |
| `read_setting`   | `setting`                                       | Stores a setting: a choice as its result, an on/off setting as 1 or 0                                |
| `stop`           |                                                 | Ends the flow it is in; later steps of that flow do not run                                          |
| `create_object`  | `object`, `steps`†, `share_tags`†               | Creates one object and runs its steps for it (see §7)                                                |
| `name`           | `subject`† (`settlement` or `person`)           | Generates a name with the naming scheme that applies                                                 |
| `builtin`        | `handler`, `config`                             | A stage the application provides (see §8)                                                            |

† optional.

**Stored results.** A `roll`, `set_result`, `secondary_roll` or `set_value`
stores its outcome under its step ID: the result, and its number (a result's
`value`, or a roll's total). Later conditions (`result_is`, `value_in`) and
counts (`value_of`) read it. In the start flow, each table stage stores its
result under its step ID too.

**What lasts for the whole generation.** Objects created, and modifiers an
`apply_modifier` step applied, carry over to later stages (they are read
back from the earlier stages' records, so a reroll of an earlier stage
invalidates the later ones that depended on it). Object numbering carries
over too. A `remove_tag` in a flow stage removes the tag for later stages;
inside an object or a For Each it only affects that object or iteration.

**For Each.** `for_each` runs its steps once for each object of its type
made so far, in order, with that object's own tags active. With
`"pairs": true` it runs once per unordered pair (1 & 2, 1 & 3, 2 & 3 …),
with no object's tags: one relationship per unique pair of people, for
example. Each iteration has its own stored results.

**Recursion.** A flow that runs itself, directly or through others, is an
error unless every `run_flow` in the loop has a `max_depth` (1–16). Such a
step runs its flow at most `max_depth` levels inside itself, then is
skipped quietly (recorded as `recursion_limit`). Validation warns that the
loop is limited (`limited_recursion`).

**Stop.** `stop` ends the flow it is in. Inside a subflow it ends only the
subflow; the caller carries on.

**Repeat counts:**

- `{ "times": 3 }`: a fixed number;
- `{ "value_of": { "step": "size", "add": -1 } }`: the number an earlier step
  stored, plus `add` (never below zero);
- `{ "dice": "1d4" }`: rolled with physical dice; the roll is recorded.

**Unique results.** A result marked `unique` (with a `fallback`) is given at
most once per flow run: rolling it again gives its fallback, never a reroll,
so the outcome follows from the dice alone. A subflow has its own run.

**The start flow.** The content's `start` names the flow a generation runs.
Each of its top-level steps is a **stage**: shown, rolled, chosen and edited
as before.

- A `roll` step is a table stage, exactly as before. Its `when` skips it,
  and its `modifiers` apply to its roll.
- A `name` step for the settlement is the Name stage.
- A `builtin` step is a built-in stage (§8).
- Any other step (a repeat, branch, object, subflow …) is a **flow stage**.
  It is rolled (and rolled again) as a whole. Its record (every roll and
  object) is shown under the stage in both interfaces, and saved with the
  generation.
- `sub_stage: true` shows a stage with the one before it, as before.

A flow stage is invalidated (and shown with its previous result) when the
tags or stored results it ran on change, and skipped or reopened when its
`when` changes, like a table stage.

The bundled pipeline is `flow.settlement` in `content/00-pipeline.json`.
Content in the older format, with a `sequence` and no `start`, still works:
its sequence is read as the steps of a start flow.

## 7. Objects

An object type describes a kind of generated thing:

```json
{
  "id": "object.person",
  "name": "Person",
  "label": "Person {n}",
  "id_prefix": "person",
  "fields": [
    { "id": "species", "name": "Species" },
    { "id": "age", "name": "Age" }
  ]
}
```

`create_object` makes one object and runs its steps for it:

```json
{
  "id": "people",
  "kind": "repeat",
  "count": { "times": 3 },
  "steps": [
    {
      "id": "person",
      "kind": "create_object",
      "object": "object.person",
      "steps": [
        { "id": "species", "kind": "roll", "table": "table.species", "field": "species" },
        { "id": "age", "kind": "roll", "table": "table.age", "field": "age" },
        { "id": "name", "kind": "name", "subject": "person" }
      ]
    }
  ]
}
```

- Each object gets a stable ID from its type and number (`person-01`,
  `person-02` …), never from display text.
- A step's `field` fills that field of the object. A field must be one the
  type declares.
- An object's results emit tags into the **object's own** scope: they shape
  the object's later rolls (e.g. its species choosing its name list) but do
  not become the generation's tags, unless the step says
  `"share_tags": true`.
- A `name` step inside an object names it; otherwise it is called by its
  type's label (`Person 1`).

## 8. Built-in steps

Some settlement stages have their own editing screens (each figure's name
parts, each district's count, each role). They stay application-provided
**built-in steps**, but everything they use is configured in content:

| `handler`           | Stage             | `config`                                                                                        |
| ------------------- | ----------------- | ----------------------------------------------------------------------------------------------- |
| `authority_roles`   | Role Allocation   | `authority`: the Authority table, size table and single/group tags. Role tables: `role_schemes` |
| `authority_figures` | Authority Figures | `authority`, `fields` (`id`, `label`, `table` each), `relationship_table`                       |
| `notable_locations` | Notable Locations | `size_table`, `type_table`, `guaranteed` (`when`, `id`, `type`, `subtype`, `spare_rooms`)       |

Built-in steps run only as top-level steps of the start flow. Their
configuration is validated like any content (tables and tags must exist,
the guaranteed location's type must be a result of the type table).

Bare Minimum Buildings is the guaranteed location's `when`:
`{ "setting_on": "app.bareMinimumBuildings" }`.

## 9. Planned and inactive rules

- `"status": "planned"` on a tag effect or a flow step stores it as a design
  note. It never runs, and its references (e.g. a table that does not exist
  yet) are not checked. Change it to active once its targets exist.
- `"active": false` on a flow step (or a table) skips it, but it is still
  checked.
- `intent` effects (a direction without approved numbers) can only be
  planned. Once numbers are approved, an intent can be replaced by a
  modifier effect, e.g. "increase Economy" by `shift` with `steps: 1`.

## 10. Validation

Content is validated before it can be used, and every customisation is
checked before it is saved; an edit that would add an error is refused with
the reasons. New checks:

| Code                         | Detects                                                                                                                                                            |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `missing_shared_result`      | A table entry listing a shared result that does not exist                                                                                                          |
| `missing_scale`              | A table naming a scale that does not exist                                                                                                                         |
| `missing_setting`            | A condition testing a setting that does not exist                                                                                                                  |
| `missing_flow`               | A `run_flow`, or the `start`, naming a flow that does not exist                                                                                                    |
| `missing_object_type`        | A `create_object` naming an object type that does not exist                                                                                                        |
| `missing_step`               | A condition or count reading a step that is neither earlier nor a stage                                                                                            |
| `missing_field`              | A step setting a field its object type does not declare                                                                                                            |
| `field_outside_object`       | A step setting an object field outside any object                                                                                                                  |
| `flow_cycle`                 | Flows that run each other in a loop with no `max_depth` on the way                                                                                                 |
| `limited_recursion` ⚠        | Deliberate recursion, limited by `max_depth` (a warning)                                                                                                           |
| `invalid_recursion_depth`    | A `max_depth` of 0 or above the subflow nesting limit (16)                                                                                                         |
| `unreachable_step` ⚠         | A step after a Stop, a case after one that always applies, or a step taken out of the flow (detached)                                                              |
| `impossible_condition`       | A test that can never hold: a result the step cannot give, an empty `any`, `min` above `max`, a test and its opposite in one `all`, a step that stores no result … |
| `empty_loop` ⚠               | A repeat of 0, a loop with no steps, or a For Each over a type nothing creates                                                                                     |
| `invalid_modifier`           | Shift 0, a shift past ±100, or a Plus/Minus amount that is not 1–1000                                                                                              |
| `modifier_needs_scale`       | Shift or Cap on a table without an ordered scale                                                                                                                   |
| `modifier_needs_dice`        | Plus or Minus on a table that is not rolled                                                                                                                        |
| `invalid_count`              | A repeat that could run more than 100 times                                                                                                                        |
| `unknown_builtin`            | A built-in step this build does not provide                                                                                                                        |
| `invalid_builtin_config`     | A built-in step whose `config` cannot be read                                                                                                                      |
| `builtin_not_top_level`      | A built-in step outside the top level of the start flow                                                                                                            |
| `reserved_setting_id`        | A content setting using the `app.` prefix                                                                                                                          |
| `invalid_setting_default`    | A choice setting whose default is not one of its options                                                                                                           |
| `condition_needs_flow`       | A `result_is` or `value_in` test outside a flow                                                                                                                    |
| `condition_not_supported`    | A `when` on the Name stage or a built-in stage                                                                                                                     |
| `duplicate_id`, `invalid_id` | Duplicate or malformed IDs of any framework item, step, field or option                                                                                            |

⚠ marks warnings; the rest are errors.

Missing tables, tags and results in flows use the existing
`missing_table`, `missing_tag` and `broken_result_reference` codes.

## 11. Defaults, customisations and saved generations

- **Bundled defaults** (`content/`) are compiled in and never changed at
  runtime.
- **Customisations** of shared results, scales, settings, object types and
  flows are stored in the user's override document beside table and tag
  customisations. Each item is customised as a whole:
  - `replace`: the user's version of a bundled item (with a fingerprint of
    the default it replaced, so a changed default is reported);
  - `delete`: a bundled item the user deleted;
  - `custom`: a user item, with a `user.` ID.

  Reset restores the installed default. Both interfaces reach these through
  the core service (`framework_items`, `framework_item`,
  `save_framework_item`, `delete_framework_item`, `reset_framework_item`).

- **Saved generations are immutable snapshots.** A flow stage's record
  copies every label, result, die and object at generation time
  (`snapshot.stages[].flow`). Later content changes never alter it; only an
  explicit edit or reroll replaces it.

## 12. Limits and recursion protection

| Limit                         | Value              | Where                                               |
| ----------------------------- | ------------------ | --------------------------------------------------- |
| Repeats of one `repeat`       | 100                | checked by validation and at run time               |
| Subflow nesting               | 16                 | at run time (loops are also rejected by validation) |
| Deliberate recursion          | `max_depth` (1–16) | per `run_flow` step                                 |
| Steps in one flow run         | 10,000             | at run time                                         |
| Table rolls in one generation | 1,000              | as before (routing loops)                           |
| Rerolls of one table          | 1,000              | as before                                           |

A run that reaches a limit stops with a readable error and changes nothing.
