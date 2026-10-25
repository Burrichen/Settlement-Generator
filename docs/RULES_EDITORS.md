# Tags, Settings and the Modifier Builder

Both interfaces edit tags (Settings › View Tags), generation settings
(Settings › View Generation Settings) and modifiers without JSON. The core
decides and words everything: what an effect does, what depends on what,
and whether an edit is safe. So the GUI and the terminal behave the same.

| Core                           | Does                                                                 |
| ------------------------------ | -------------------------------------------------------------------- |
| `explain::explain_effect`      | One effect in plain English, with a roll preview and what it affects |
| `explain::explain_modifier`    | A flow step's modifier, the same way                                 |
| `explain::roll_preview`        | Several modifiers on one roll, step by step                          |
| `dependencies::tag_usages`     | Everything that gives, checks, adds or removes a tag                 |
| `dependencies::setting_usages` | Everything that tests or reads a setting                             |
| `AppService::explain_tag`      | A tag draft in words: each effect, combined previews, informational  |
| `AppService::*_setting`        | New, check, save and safe delete of setting definitions              |
| `AppService::delete_tag`       | Refused, naming the uses, while anything still uses the tag          |

## Tags

A tag is a label generation can give (a result's tags, an Add Tag effect or
flow step). In the tag editor you can:

- **create** a tag (a `user.` ID), **rename** it and change its description.
  Everything refers to the ID, so renaming never breaks a reference;
- **disable** it: it is never applied and its effects never run;
- **attach effects** with the Modifier Builder (below);
- **inspect its dependencies**, grouped: _Given by_ (results), _Added by_
  (tags and flow steps), _Removed by_ (flow steps) and _Checked by_ (table,
  result, variant and follow-up conditions, flow step and case conditions,
  step modifiers, built-in stage settings);
- **delete it safely**: deletion is refused, naming every use, while
  anything still uses it. Disable it instead to keep it but switch it off;
- **reset** a customised default to the installed version (a deleted default
  can be restored from the list).

A tag may change table weighting, enable or disable results, steer flow
branches (any condition can test it), trigger modifiers, or do nothing but
label the settlement. A tag with no active effects is shown as
**informational only**.

## Generation settings

Settings › View Generation Settings lists every setting the content defines.
A setting is one of:

| Kind         | Offered in Settings as            | Flow and table conditions can test        |
| ------------ | --------------------------------- | ----------------------------------------- |
| On/off       | a switch                          | is on / is off                            |
| Number       | a number (←→ steps it in the TUI) | is exactly / between / at least / at most |
| Dropdown     | a choice of options               | is / is not an option                     |
| Multi-select | ticked options                    | includes / does not include               |
| Text         | text (up to 200 characters)       | is / is not a text                        |

The editor sets the name, description, kind, options (each option's ID is
fixed when it is added, so renaming it keeps conditions working), default,
number range, and **Planned** (documented, not offered: generation uses the
default). It lists every **dependency** (conditions that test the setting,
Read Setting steps, built-in stages configured with it) and refuses to
delete a setting while anything uses it.

**Bare Minimum Buildings** is one of these: a bundled on/off setting the
Notable Locations stage tests. It appears in Settings like any other
content setting, keeps the choice made before, and cannot be deleted while
the pipeline uses it.

## The Modifier Builder

Effects are grouped by what they do:

| Group                            | Effects                                                                                                                 |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Change the roll                  | Shift (+1, +2, −1, −2 or any number), Plus X, Minus X, Cap at Highest, Cap at Lowest, Change dice, **Change weighting** |
| Change which results can come up | Restrict results (disable some, or allow only some), **Enable results**, Use variant, Redirect table                    |
| Trigger more generation          | Route to table, **Run subflow**, Add tag, Secondary roll                                                                |
| Design notes                     | Intent (no numbers yet)                                                                                                 |

Each effect is **Active** or **Planned**. A planned effect is shown and
documented ("Planned, not run: …") but never executes, and what it names
does not need to exist yet.

Every effect is explained in plain English as it is built, with what
happens to a roll step by step:

- Plus 2 on a 1d12 table: _Roll 1d12 → add 2 → cap at 12 → resolve
  result._
- Shift +1 on Crime: _Resolve Crime → shift result up 1 step → cap at
  Rampant._
- Rich twice as likely on a 1d12 table where it has half the faces:
  _Lay Wealth out again on 1d12 → Rich: 50% → 67% → Roll 1d12 → resolve
  result._

When a tag puts several modifiers on one table, the tag editor also shows
them **together on one roll**, in the order the engine applies them (Plus
and Minus first, then Shift, then Cap).

The flow editor's modifiers (on a Roll Table step, and Apply Modifier
steps) are explained the same way.

### Weighting stays a physical roll

Change weighting never picks at random in software. While the tag is
active, the table's results are laid out again on its **own dice** so that
the result's chance matches as closely as the dice allow (the same layout
the Dice Assistant would give). The roll is still a physical roll of the
same dice.

### Run subflow

A tag with Run subflow runs a flow right after the flow step that gave it
(once per tag; a flow already running is never started again inside
itself). It only runs inside flows: if a top-level table stage of the
pipeline can give the tag, validation warns, because there it would not
run.

## Dependency inspection, at a glance

| Item     | Shows                                                                   |
| -------- | ----------------------------------------------------------------------- |
| Tag      | Given by, Added by, Removed by, Checked by; and each effect's _Affects_ |
| Setting  | Tested by (conditions), Read by (flow steps), Used by (built-in stages) |
| Modifier | Affects (the table and results), and the roll preview                   |
