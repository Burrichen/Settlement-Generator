# Settlement naming

Settlement naming was released in v0.1.1. It is data-driven: naming content is
JSON in `content/`, and generation reuses the same tables, physical dice,
validation, overrides and snapshots as every other stage. Nothing about a
particular region is written in code.

This document covers how naming works, the file format, the dice rules, and
how to edit naming content. Ordon's pack (five methods, approved names) has
its own guide: [ORDON_NAMING.md](ORDON_NAMING.md).

## How a name is chosen

The chain is Setting Profile → Region → naming scheme → naming method → name.

1. **Name stage.** The generation sequence has a **Name** stage straight after
   Region:

   ```json
   { "naming": true, "name": "Name" }
   ```

   The Name stage works in Automatic, Manual Incremental and Manual One-table,
   and when editing a saved settlement.

2. **Scheme.** As soon as Region is known, the core picks the first **naming
   scheme** whose `when` condition holds over the tags generated so far. Ordon's
   scheme applies when both `world.kurovia` and `region.ordon` are active.

3. **Method.** The scheme's **method table** is rolled with its dice. It can
   also be chosen; this is "Choose method". Each result of that table is a
   **naming method**.

4. **Parts.** The method's **component tables** are rolled in order with their
   dice.

5. **Join.** The rolled parts are joined by the method's **join rule**, after
   its **spelling exceptions** are checked.

Two rules apply throughout:

- **A typed name is always possible**, with or without naming data. A typed
  name is never invalidated by later changes, and Re-roll keeps it.
- **No naming data means no generated name.** Kurovia's seven other regions and
  every other Setting Profile have no scheme yet, so the Name stage is
  **skipped**: the core never fabricates a name. The user can type one, or name
  the settlement when saving.

Changes made on another stage do the following:

- **Changing Region** re-checks the name. A generated name whose scheme or
  rules changed is invalidated; the old name stays visible as "was …". With no
  scheme, the stage is skipped again.
- **Rerolling the name** changes nothing else in the settlement.
- **Naming never adds or changes settlement tags**, including in God Mode.
- **No Age, Origin or Environment naming modifiers exist.** Future content can
  add them as method eligibility conditions.

## Files

Each region's naming data is a folder. Naming is the one exception to the
"one table per file" rule: a name pack may keep a pool's culture or sub-pool
tables in the same file as the pool (the human given-name files, for
example). Everything outside `naming/` folders still has one table per file.

| Path                               | Contains                                                                                                                                                                   |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `content/00-sequence.json`         | The pipeline, including the `{ "naming": true }` Name stage                                                                                                                |
| `content/kurovia/naming/<region>/` | One file per table (`01-method.json`, `02-first-element.json`, …) and the region's own `naming-schemes.json`. Ordon has ten tables; see [ORDON_NAMING.md](ORDON_NAMING.md) |

Ordon, Bratislor, Halingo, Hjartheland and Nareth have expanded packs;
Skgurr, Shinkira and Targoth are still small. Which settlement and personal
name sets have not been revamped yet is tracked in
[NAMING_BACKLOG.md](NAMING_BACKLOG.md).

All eight Kurovian regions have a naming scheme. No generic scheme exists
yet, so other Setting Profiles skip the Name stage.

Every region follows the same predictable pattern. For region `<key>`:

- scheme `naming.kurovia.<key>`;
- tables `table.naming.<key>.method`, `.firstElement`, `.secondElement`,
  and further tables for any other methods the region has;
- method results such as `geographic` and `founder`, plus the region's own.

Component result IDs are the lower-case entry, e.g. `khar` or `varesh`.

Every `.json` file under `content/`, including subfolders, is merged in path
order at build time. Keep authoring material (previews, catalogues, archived
lists) outside `content/`, e.g. in `authoring/`, or it is bundled too. A new region gets its own folder, for example
`content/kurovia/naming/halingo/`, with one file per table and a
`naming-schemes.json`. No registry or code change is needed.

## Person naming

A naming scheme may name a person instead of the settlement:
`"subject": "person"` (the default is `"settlement"`). Person schemes are used
for Authority Figures. The scheme is chosen by the first `when` condition
that holds over the settlement's tags **plus the figure's own tags**, such
as `species.human` and `gender.woman`, so names route by species and, where
the content wants, by gender. They never apply to the settlement's Name
stage, and settlement schemes never name a person.

Person naming uses the same methods, component tables, join rules and dice
as settlement naming.

### Species names (Authority Figures)

Authority Figure names route **by species first**. There is no universal
name table:

```
Species → Gender (where the species uses it) → Given name
        → Family / clan / house / lineage / byname (where the species uses one)
        → Final name
```

- **Temporary content.** The species are the current, temporary Kurovia list
  (see [CONTENT.md](CONTENT.md)), and so are their names. All of them are
  **original** and editable. Species can be added or removed without any
  schema change: add a file with tables and schemes, or delete one.
- **Files.** `content/people/naming/<species>/`, a folder per species with
  one file per table and a `naming-schemes.json`. For a
  species key `<key>` (the `species.<key>` tag), its tables are
  `table.personName.<key>.method`, its given-name pools and its second-name
  table, and its schemes are `naming.person.<key>…`.
- **Gendered pools.** Human, High Elf, Wood Elf, Dark Elf, Dragonborn and
  Dwarf have masculine, feminine and neutral given-name pools (20 names each,
  1d20). Three schemes per species route them: `gender.man` → masculine,
  `gender.woman` → feminine, and a last, species-only scheme → neutral. The
  neutral pool also names any gender added later, so a new gender never falls
  through to another species.
- **One pool.** Eladrin, Yuan-Ti, Firbolg and the four Genasi use one
  given-name pool for every gender: 50 names on 1d100, two faces each.
- **Second names.** Human family names, High Elf and Dark Elf house names,
  Wood Elf kin names, Dwarf and Dragonborn clan names, Eladrin house names,
  Yuan-Ti lineage names, and Earth and Fire Genasi bynames: 20 each, on 1d20.
  Firbolgs and Air and Water Genasi go by a single name.
- **Patterns.** Each species has its own method table (1d4). Most have one
  pattern. Dragonborn use given name + clan name on 1–3, and the clan name
  first (formal address) on 4. A method's optional `parts`
  (`[{ "id": "given", "label": "Given name" }, …]`, one per component)
  name what each component is.
- **What is saved.** A generated name stores the full name, the figure's
  species at the time, the scheme and method, each named part's result ID
  (e.g. `given`: `mila`, `family`: `varen`) and every roll with its dice.
  Saved settlements keep it exactly, whatever later edits do to the tables.
- **Manual modes.** A name can be rolled again, typed, or put together part
  by part: each part offers only its own species (and gender) table. God Mode
  lifts result eligibility within that table; it never merges species pools.
  Changing a figure's species or gender routes its name afresh (a typed name
  stands).
- **No name table.** A species with no scheme (for example one a user adds)
  is never given another species' names: the figure shows "No name table
  configured for <species>." and can be named by hand.
- **Later layers.** Kurovian cultural or regional naming can be added on top
  later, as person schemes whose `when` also checks a region tag, placed
  before the species scheme. No regional modifiers exist yet.

## Format

A naming file is an ordinary partial content set. Its `tables` are normal
tables: they appear in **Settings → View Tables** and can be customised like
any other table. Its `naming_schemes` connect those tables.

```json
{
  "schema_version": 1,
  "tables": [
    {
      "id": "table.naming.ordon.method",
      "name": "Ordon Naming Method",
      "dice": "1d100",
      "results": [
        { "id": "geographic", "name": "Old English & Landscape", "range": [1, 42] },
        { "id": "founder", "name": "Founders, Manors & Estates", "range": [43, 62] }
      ]
    }
  ],
  "naming_schemes": [
    {
      "id": "naming.kurovia.ordon",
      "name": "Ordon",
      "when": { "all": [{ "has_tag": "world.kurovia" }, { "has_tag": "region.ordon" }] },
      "method_table": "table.naming.ordon.method",
      "methods": [
        {
          "result": "geographic",
          "components": ["table.naming.ordon.firstElement", "table.naming.ordon.secondElement"],
          "join": { "style": "compound" },
          "exceptions": [
            { "results": ["ash", "ham"], "name": "Asham", "note": "Historical-looking contraction" }
          ]
        },
        {
          "result": "founder",
          "components": ["table.naming.ordon.founder", "table.naming.ordon.founderEnding"],
          "join": {
            "style": "curated_variant",
            "source_field": "approved_names",
            "variant_from_result_id": 1,
            "on_missing": "error"
          }
        }
      ]
    }
  ]
}
```

### Naming scheme

| Field          | Meaning                                                                     |
| -------------- | --------------------------------------------------------------------------- |
| `id`           | Stable ID, e.g. `naming.kurovia.ordon`. Never shown; never rename it        |
| `name`         | Display name, e.g. "Ordon"                                                  |
| `description`  | Optional                                                                    |
| `when`         | A [condition](DATA_MODEL.md) over tags; the first scheme that holds is used |
| `method_table` | The table whose results are the methods                                     |
| `methods`      | One entry per result of the method table                                    |

### Naming method

A method has two halves:

- **The method table's result** holds the method's stable ID, name and
  description. Its **faces are its weight**. Its optional **eligibility
  condition** is its tag condition; God Mode lifts that condition, exactly as
  on any table.
- **The method entry** in `methods` holds the rest:

| Field        | Meaning                                                                            |
| ------------ | ---------------------------------------------------------------------------------- |
| `result`     | The method table result this method belongs to                                     |
| `components` | Tables rolled in order; each gives one part                                        |
| `join`       | How parts are joined (below)                                                       |
| `exceptions` | Exact spellings for particular combinations of component results, in order (below) |

Adding a method means adding a result to the method table, giving it faces,
and adding a matching `methods` entry. A scheme may have any number of
methods, and a method any number of components.

### Join rules

A join rule writes the parts in order. It puts `separator` between them, adds
a possessive suffix where the style says so, and then capitalises the result
according to `case`.

| Field          | Values                                                         | Meaning                                                                                                                                                                                                         |
| -------------- | -------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `style`        | `compound` (default), `space`, `possessive`, `curated_variant` | See below                                                                                                                                                                                                       |
| `separator`    | text, default none                                             | Written between parts, e.g. `" "` for two-word names                                                                                                                                                            |
| `case`         | `sentence` (default), `words`, `as_written`                    | `sentence`: first letter upper case, the rest lower case. `words`: the same for each space-separated word. `as_written`: unchanged                                                                              |
| `replacements` | list of `{ "from": …, "to": … }`, default none                 | Exact substitutions applied in order to the joined name, ignoring case, before capitalisation. They let an editor add phonetic rules, e.g. `{ "from": "aa", "to": "a" }`. An empty `from` is a validation error |

| Style             | Rule                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `compound`        | The parts as they are                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `space`           | The parts separated by one space, each word capitalised: the same as `compound` with `"separator": " "` and `"case": "words"`. An explicit `separator` or `case` still applies                                                                                                                                                                                                                                                                        |
| `curated_variant` | Not a join: the source part's result holds approved final names in `approved_names`, keyed by the variant part's result ID, and that name is used **exactly as written** (no separator, case, replacement or possessive). `variant_from_result_id` (zero-based, usually `1`) picks the variant part; `source_from_result_id` (default `0`) the source. A missing name is a validation error, never a fallback. See [ORDON_NAMING.md](ORDON_NAMING.md) |
| `possessive`      | `suffix` is written after a part when the next part's result ID is listed in `before`, or before every next part when `before` is absent. If the part already ends in "s" (ignoring case), `after_s` is written instead when given. Without `after_s`, a part that already ends with `suffix` gets nothing added                                                                                                                                      |

A method with **one component** gives that part exactly as written, with no
capitalisation or replacement, so complete names such as "St. Cuthred" or
"Vara Talen" stay as authored. A join's optional `description` is an
editor's note with no effect.

Each rule belongs to a method, never to the whole application, and each region
chooses its own:

| Region / method                    | `join`                                                                                          | Examples                                          |
| ---------------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| Ordon Geographic                   | `{ "style": "compound" }`                                                                       | Alderwick, Brackenbury                            |
| Ordon Founder, Industry, Transport | `{ "style": "curated_variant", "source_field": "approved_names", "variant_from_result_id": 1 }` | Oswaldwick, Foulden, Market Cross                 |
| Ordon Inherited                    | `{ "style": "compound" }`, one component                                                        | Netherleigh, St. Cuthred                          |
| Halingo Geographic                 | `{ "style": "compound", "separator": " ", "case": "words" }`                                    | Khar Well, Cinder Ridge                           |
| Halingo Founder                    | `{ "style": "possessive", "suffix": "'s", "after_s": "'", "separator": " ", "case": "words" }`  | Varesh's Well; a founder ending in s: Tamas' Well |
| Skgurr Geographic                  | `{ "style": "compound" }` (Skgurr's only method)                                                | Dunwick, Inverness, Muckleskerry                  |
| Shinkira Geographic                | `{ "style": "compound" }`                                                                       | Kirisawa, Kuroyama                                |
| Shinkira Founder                   | `{ "style": "compound" }`: no apostrophe, no English possessive                                 | Takedamura, Mizunosato                            |
| Bratislor Geographic               | `{ "style": "compound" }`                                                                       | Drevgrad, Morava, Vranova                         |
| Bratislor Founder                  | `{ "style": "compound" }`: no English possessive                                                | Vorelgrad, Radanov, Branikdol                     |
| Hjartheland Geographic             | `{ "style": "compound" }`                                                                       | Nordvik, Eirheim, Steindal                        |
| Hjartheland Founder                | `{ "style": "compound" }`: no possessive s                                                      | Eirikheim, Signevik, Hakongard                    |
| Nareth Geographic                  | `{ "style": "compound", "replacements": [] }`                                                   | Naroneth, Korivai, Lumari                         |
| Nareth Founder                     | `{ "style": "compound", "replacements": [] }`                                                   | Talarien, Ivaramar, Namarvai                      |
| Targoth Geographic                 | `{ "style": "compound" }`                                                                       | Karhold, Skarnwatch, Vathgate                     |
| Targoth Founder                    | `{ "style": "possessive", "suffix": "'s", "separator": " ", "case": "words" }`                  | Varok's Gate, Thren's Watch, Voran's Rest         |

The fields `separator`, `case`, `after_s` and `replacements` were added for
v0.1.1's regional packs; `space` and `curated_variant` for the Ordon Quality
Pass v2. Each is additive: every existing style and default behaves as before.

**Spelling exceptions** are checked before the join rule, for every style.
When the component results match, in order, the exception's `name` is used
**exactly as written**. An exception lists either:

- `results`: component **result IDs**. Preferred: renaming a component never
  breaks it. `{ "results": ["ash", "ham"], "name": "Asham", "note": "…" }`
- `components`: component **display names**, the form some packs are
  supplied in: `{ "components": ["Ash", "ham"], "output": "Asham" }`
  (`output` is read as `name`). Validation requires each name to match
  exactly one result, so a renamed component is reported, never ignored.

`note` is optional and has no effect. Ordon has 17 exceptions, e.g.
Ash + ham → Asham, Hill + ley → Hildley and Mill + ley → Milnley.

There are no hidden spelling changes and no rerolls to improve a name. An
awkward result stays visible and can be rerolled, or replaced with a typed
name.

## Dice rules

- Every naming table uses real dice: d4, d6, d8, d10, d12, d20 or d100, or a
  sensible combination of them.
- Ranges must cover every possible total **exactly once**. Validation rejects
  gaps and overlaps, and View Tables shows them.
- **Weight = faces.** A result with 4 faces on a d100 has a 4% chance.
- Rolls are reproducible: the same dice give the same name. Every die
  is recorded: the method roll, then one roll per component.

Ordon's weights and tables are listed in [ORDON_NAMING.md](ORDON_NAMING.md).

## Regional packs: Halingo, Skgurr, Shinkira

These three packs are **provisional sample data**, written as illustrative
starting points. They are **not** additions to official Kurovian lore. Halingo
and Shinkira use the starter layout below, and every result within a table is
equally likely. Skgurr has its own layout (see below):

| Table          | Dice | Layout                                                       |
| -------------- | ---- | ------------------------------------------------------------ |
| Naming Method  | 1d10 | Geographic Compound 1–7 (70%), Historical Founder 8–10 (30%) |
| First Element  | 1d12 | 12 entries, one face each                                    |
| Second Element | 1d12 | 12 entries, one face each                                    |
| Founder        | 1d12 | 12 names, one face each                                      |
| Founder Ending | 1d6  | 6 entries, one face each                                     |

None of the three packs has spelling exceptions yet. Add them with
`exceptions` if an awkward combination needs a fixed spelling; nothing is
ever transformed silently.

### Halingo (`halingo.json`, routed by `region.halingo`)

Arid frontier settlements, harsh landscapes, resource extraction and desert
trade. The names are dry, understated and slightly harsh.

- **Geographic Compound:** two words, each capitalised, e.g. Khar Well, Cinder
  Ridge, Silt Crossing, Varr Rest.
- **Historical Founder:** founder + English possessive + space + feature, e.g.
  Varesh's Well, Karven's Crossing, Saren's Rest.
- **Founders ending in s.** A founder whose name ends in "s" takes only an
  apostrophe, with no extra "s" (Tamas' Well). This is Halingo's
  `"after_s": "'"`. No approved Halingo founder currently ends in "s"; the rule
  applies to names added later. It applies to Halingo only.

### Skgurr (`skgurr.json`, routed by `region.skgurr`)

Old Scottish and coastal naming: wind-beaten ports, highlands, sea caves and
isolated shorelines. The names are grounded and weathered.

Scottish Gaelic, Scots and Norse-influenced geographical compounds. Skgurr
has **no founder, survival or misfortune naming methods**.

| Table          | Dice  | Layout                                                    |
| -------------- | ----- | --------------------------------------------------------- |
| Naming Method  | none  | Geographic Compound only: no die is rolled                |
| First Element  | 1d100 | 30 entries: 4 faces each for the first 10, 3 for the rest |
| Second Element | 1d100 | 30 entries: 4 faces each for the first 10, 3 for the rest |

A method table with no dice (`"selection": "choose"`) and only one result
that can be taken needs no roll: that method is always used, and the name
rolls only its component tables. Skgurr's method table works this way.

- **Geographic Compound:** one word, no separator, no spelling exceptions,
  e.g. Dunwick, Inverness, Greyhaven, Strathmuir, Sronvoe, Muckleskerry.

### Shinkira (`shinkira.json`, routed by `region.shinkira`)

Romanised, Japanese-inspired components for forest and mountain settlements.
They are used as temporary creative data. **They are not a complete or
historically accurate Japanese naming system.**

- **Geographic Compound:** one word, e.g. Kirisawa, Mizumura, Kuroyama,
  Hiraoka.
- **Historical Founder:** a family or founder name joined directly to a
  settlement element. There is no apostrophe and no English possessive, e.g.
  Takedamura, Mizunosato, Uedashinden. Shinkira's join rules are its own and
  can be changed without affecting any other region.

### Bratislor (`bratislor.json`, routed by `region.bratislor`)

Old Eastern and Central European naming traditions, with a gothic atmosphere
of decayed communities and older settlements shaped by the Haze. The names are
sombre and plausible rather than overtly frightening. **This is a fictional,
language-inspired set, not a claim of accurate Slavic morphology.**

- **Geographic Compound:** one word, e.g. Drevgrad, Morava, Vranova, Velgrad.
- **Historical Founder:** founder + settlement element, joined directly with
  no English possessive, e.g. Vorelgrad, Radanov, Branikdol.
- **Exceptions:** both methods include an empty `exceptions` list for awkward
  combinations, such as doubled vowels (Novo + ovo). Fill it in when an
  editorial spelling is chosen; nothing is changed automatically.

### Hjartheland (`hjartheland.json`, routed by `region.hjartheland`)

Scandinavian and Old Norse naming traditions: cold highlands, glacial country
and the Forever Ice. The names are traditional, sturdy and geographically
meaningful.

- **Geographic Compound:** one word, e.g. Nordvik, Eirheim, Steindal, Skognes.
- **Historical Founder:** founder + ending, joined directly, e.g. Eirikheim,
  Signevik, Hakongard. There is no possessive s.
- **Spelling:** the shipped lists are plain ASCII (Bjorn). If an editor uses
  letters such as ø or æ, they are joined and capitalised correctly.

### Nareth (`nareth.json`, routed by `region.nareth`)

A rainforest and wetland region of extensive waterways, whose naming identity
is **not established yet**. The entries are **original, pronounceable
placeholder sounds** for an editor to replace:

- They are **not** Nareth vocabulary.
- No meanings are implied.
- They do not imitate any real language.

- **Geographic Compound:** one word, e.g. Naroneth, Korivai, Saqmar, Lumari.
- **Historical Founder:** founder + ending, joined directly, e.g. Talarien,
  Ivaramar, Namarvai.
- **Phonetic rules:** both methods ship an empty `replacements` list. Phonetic
  rules can be added there later as data, for example
  `{ "from": "aa", "to": "a" }`, with no code change.

### Targoth (`targoth.json`, routed by `region.targoth`)

High-mountain, remote and partly abandoned country. The names sound older,
austere and durable, as if they survived from ancient routes, strongholds and
inhabited valleys. The region's canonical name is Targoth; the older working
name Targon is not used.

- **Geographic Compound:** one word, e.g. Karhold, Skarnwatch, Vathgate,
  Drelstone.
- **Historical Founder:** founder + 's + space + feature, e.g. Varok's Gate,
  Thren's Watch, Voran's Rest. A founder ending in "s" still takes 's, because
  Targoth has no `after_s`. This rule is Targoth's own.

## Editing naming content

### In the app (any user)

**Settings → View Tables** lists every naming table. Users can:

- rename methods, elements, founders and endings;
- change dice and faces, which changes the weights;
- add or remove entries;
- add a tag condition to a method, as result eligibility;
- edit **approved names**: a table read by a `curated_variant` method shows
  one field per variant on each result (e.g. Oswald: Variant A, Variant B);
- edit each method's **joining rule and spelling exceptions**, in the
  **Naming methods** section of the scheme's method table. Each method is
  saved and reset on its own.

The coverage strip shows every face, so gaps and overlaps are visible before
saving. Customisations are stored as user overrides: updates never overwrite
them, and **Reset** restores the default. A change that would make the
content invalid (a missing approved name, an exception naming an unknown
result) is refused with the reason.

Scheme routing (`when`), a method's components and new methods live only in
the content files.

### In VS Code (content authors)

1. Open the table's file in `content/kurovia/naming/ordon/` (for example
   `02-first-element.json`), or `naming-schemes.json` for the scheme.
2. Edit as needed:
   - **Rename an entry:** change its `"name"`. Keep its `"id"`; spelling
     exceptions and possessive endings refer to IDs.
   - **Change weights:** edit the `"range"` values (`[min, max]`, or a single
     number). Keep every total from 1 to the die's size covered exactly
     once.
   - **Add an entry:** add a result with a new unique `"id"` and give it
     faces taken from neighbouring entries.
   - **Change a method's weight:** edit the method table's ranges.
   - **Add a spelling exception:** add
     `{ "results": [first ID, second ID], "name": "Exact Spelling", "note": "Why" }`
     to the method's `exceptions`.
   - **Edit an approved name:** edit the source result's
     `"approved_names": { "a": "…", "b": "…" }`. Every variant needs one.
   - **Change possessive endings:** edit the method's `join.before` list of
     ending result IDs.
   - **Change how parts are joined:** edit the method's `join`. For
     example, to write Halingo's Geographic names as one word ("Kharwell"),
     change

     ```json
     "join": { "style": "compound", "separator": " ", "case": "words" }
     ```

     to

     ```json
     "join": { "style": "compound" }
     ```
3. Run `cargo test`. Validation reports any of the following, naming the file
   location:
   - gaps or overlaps in ranges;
   - unknown IDs;
   - a method-table result without a method;
   - an exception with the wrong number of parts;
   - a missing or empty approved name, naming the table, result and variant.

   `crates/settlement-core/tests/naming.rs` and `tests/ordon_naming.rs`
   check the approved Ordon lists, weights, examples and full catalogue.
   Update them when the editorial content changes on purpose.

4. Build. The content is compiled into both the GUI and the terminal
   application.

### Adding another region

Create the folder `content/kurovia/naming/<region>/` with:

- its own method table;
- its component tables (one file per table is usual, but sub-pool tables may
  share a file);
- a `naming-schemes.json` whose scheme's `when` is `world.kurovia` plus that
  region's tag.

Then add tests for its lists. The engine needs no change.

## Saved settlements

The Name stage is saved in the snapshot as a **name record** with:

- the final name;
- whether it was generated or typed;
- the scheme and method (ID and name);
- every roll: the method roll and each component roll, with dice, totals
  and result IDs, and whether each was rolled or chosen;
- how it was assembled: the join style, and any spelling exception or
  approved name used (names saved before this was recorded have none);
- the application version that produced it.

Saved names are never regenerated:

- **Changes to naming files** do not alter saved settlements.
- **Reopening a settlement to edit** restores the record exactly.
- **Settlements saved before naming existed** show no Name and save unchanged.

The name saved in the list is still the one entered when saving. The save
dialog offers the Name stage's name, which can be changed there.

## Interfaces

Both interfaces call the same core session. There is no second name
generator.

| Action                    | GUI                                                                               | Terminal                                                              |
| ------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Roll a name (or reroll)   | "Roll name" / "Reroll name" (Incremental), Roll in the grid                       | `r` on the Name stage                                                 |
| Choose a method           | Method list (Incremental), "Naming method" select (grid)                          | `Enter` on the Name stage                                             |
| Change one part           | "Name parts": a select per part, showing the name each option gives, and its Roll | `c` on the Name stage: pick the part, then roll it or choose a result |
| Type a name               | "Or type a name" form                                                             | `t` on the Name stage                                                 |
| Inspect routing and rolls | Tag View                                                                          | Tag View (notes under the grid)                                       |
