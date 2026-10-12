# Bundled Content (v0.1.0)

The approved generation content and its current state. The data itself lives
in `content/*.json`, which is merged in file-name order and compiled into both
binaries. It is never hardcoded in Rust or React.
`crates/settlement-core/tests/bundled_content.rs` checks every table below
against this specification, including each result's percentage.

Content development currently concentrates on **Kurovia**. Every Setting
Profile is kept, but no non-Kurovia content has been invented.

## Pipeline

`content/00-sequence.json`. Each stage carries its pipeline name, so the Region
stage is labelled "Region" even when Kurovia redirects it to Kurovia Region:

| #   | Stage             | Table                                                    |
| --- | ----------------- | -------------------------------------------------------- |
| 1   | Setting Profile   | `table.settingProfile` (chosen)                          |
| 2   | Region            | `table.genericRegion`, redirected by region-routing tags |
| 3   | Name              | Naming stage: the naming scheme for the region, if any   |
| 4   | Environment       | `table.environment`, redirected by Kurovia region tags   |
| 5   | Connections       | `table.connections`                                      |
| 6   | Size              | `table.size`                                             |
| 7   | Origin            | `table.origin`                                           |
| 8   | Purpose           | `table.purpose`                                          |
| 9   | Age               | `table.age`                                              |
| 10  | Economy           | `table.economy`                                          |
| 11  | Authority         | `table.authority` (Power)                                |
| 12  | ↳ Representation  | `table.authorityRepresentation` (sub-stage, conditional) |
| 13  | ↳ Authority Size  | `table.authoritySize` (sub-stage, conditional)           |
| 14  | Authority Figures | `{ "authority_figures": true }`: see below               |

Naming content (v0.1.1) is described in [NAMING_SYSTEM.md](NAMING_SYSTEM.md):

- Naming data so far, in `content/kurovia/naming/`:
  - **Ordon**, the approved content;
  - **Halingo**, **Skgurr**, **Shinkira**, **Bratislor**, **Hjartheland**,
    **Nareth** and **Targoth**, provisional samples that are not official
    lore.
- All eight Kurovian regions are named. Other Setting Profiles skip the Name
  stage, because there is no generic scheme yet. A name can be typed instead.

## Setting Profile

This was formerly called "Fantasy Profile".

`table.settingProfile` is **chosen, not rolled**, because no dice have been
specified for it. Each result emits its `world.*` tag:

- Kurovia — `world.kurovia`
- Low Fantasy — `world.lowFantasy`
- Medium Fantasy — `world.mediumFantasy`
- High Fantasy — `world.highFantasy`
- Forgotten Realms — `world.forgottenRealms`
- Horror — `world.horror`

## Region routing

- **Rule:** if no Region-routing tag applies, Generic Region is used.
- `world.kurovia` has an active `redirect_table` effect: Generic Region →
  Kurovia Region.
- `world.horror` has a **planned** redirect to `table.horrorRegion`. That
  table does not exist yet and its content is not designed. Until it is,
  Horror uses Generic Region.
- Generic Region (1d10) and Kurovia Region (1d8) are entered as specified.
  - Each Kurovia result emits its own `region.<name>` tag.
  - The generic results also emit `region.*` tags, for Tag View and future
    rules.
  - Kurovia regions are mapped to faces 1–8 in the order they were listed.

## Environment

- `table.environment` is the stage table.
- Each Kurovia `region.*` tag redirects it to that region's 1d20 table,
  `table.kuroviaEnvironment.<region>`.
- Each environment result emits an `env.*` tag.
- Environment for non-Kurovia regions has not been designed, so the stage
  table is marked **incomplete**.

## Connections

- `table.connections` uses the Standard 1d20 layout as its base. Region
  variants (selected by `region.*` tags) exist for Ordon, Hjartheland, Nareth,
  Targoth and Shinkira.
- Targoth and Shinkira have no Major Route faces.
- Bratislor uses Standard. Halingo and Skgurr use Standard until otherwise
  specified.
- There is no Track result.
- **Shinkira + Coast** is a table follow-up roll (`shinkiraCoastUpgrade`),
  part of the generic rules model:
  - **When:** `region.shinkira` and `env.coast` are both active.
  - **Trigger:** the result is Local Road or Regional Road.
  - **Roll:** 1d4. On 1–3 the original result is kept; on 4 it becomes Major
    Route.
  - The record keeps the original roll, the 1d4 and the final result.

## Size

`table.size` is the Standard Size table, rolled on **1d10**. Each result has a
20% baseline chance and emits its `size.*` tag:

| 1d10 | Result       | Tag                |
| ---- | ------------ | ------------------ |
| 1–2  | Trading Post | `size.tradingPost` |
| 3–4  | Village      | `size.village`     |
| 5–6  | Town         | `size.town`        |
| 7–8  | City         | `size.city`        |
| 9–10 | Capital      | `size.capital`     |

- Size is an **ordered scale**, marked `"ordered": true`: Trading Post →
  Village → Town → City → Capital. The results are listed smallest first, and
  their faces ascend in the same order. A future dice modifier on Size
  therefore moves the result towards a smaller or larger settlement without
  naming individual results.
- These five are the only core sizes. Fortress is deliberately deferred to
  future DLC/content. There is no Hamlet, no Small/Large variant and no
  population band.
- Age's effect on Size is a planned intent only (see below). The exact
  modifier has not been approved.

## Origin, Purpose, Age

- These are entered as specified: Origin and Purpose use 1d20, and Age uses
  1d8.
- Purpose-Built's description records that the origin equals the Purpose
  generated later.
- The Development table has not been reintroduced.

## Economy

`table.economy` is the settlement's current overall economic condition. It is
the Standard Economy table, rolled on **1d12**, and each result emits its
`economy.*` tag:

| 1d12  | Result     | Chance | Tag                  |
| ----- | ---------- | ------ | -------------------- |
| 1     | Failing    | 1/12   | `economy.failing`    |
| 2–3   | Struggling | 2/12   | `economy.struggling` |
| 4–7   | Stable     | 4/12   | `economy.stable`     |
| 8–10  | Prosperous | 3/12   | `economy.prosperous` |
| 11–12 | Wealthy    | 2/12   | `economy.wealthy`    |

- Economy is an **ordered scale**, marked `"ordered": true`: Failing →
  Struggling → Stable → Prosperous → Wealthy. Its faces ascend in the same
  order, so a future dice modifier shifts Economy up or down without naming
  results.
- Upstream modifiers (Isolated, Major Route, Trade) and the downstream effects
  of each result are planned intents; see below. **None has an approved
  strength**, so none is executed.

## Authority (Power)

`table.authority` answers one question: **who officially governs the
settlement?** It covers formal governance only. Hidden power, informal
influence, factions, guild influence short of formal rule, NPC personalities
and detailed government structure are left to later tables.

It is rolled on **1d12**. Each result has a short description and emits its
`authority.*` tag:

| 1d12 | Result                     | Tag                   |
| ---- | -------------------------- | --------------------- |
| 1–3  | Local Council              | `authority.council`   |
| 4–5  | Noble / Landed Ruler       | `authority.noble`     |
| 6    | Appointed Official         | `authority.appointed` |
| 7–8  | Merchant / Guild Rule      | `authority.guild`     |
| 9    | Religious Authority        | `authority.religious` |
| 10   | Military Authority         | `authority.military`  |
| 11   | Elders / Community Leaders | `authority.elders`    |
| 12   | Popular Assembly           | `authority.assembly`  |

- Authority is not an ordered scale.
- **Economy does not affect Authority.** A wealthy settlement may have elders,
  and a failing one may still have a noble ruler.
- Origin, Purpose and Size make some results more or less likely. These are
  planned intents with **no approved strength**; see below.

### Authority Representation and Authority Size

After Authority, two conditional **sub-stages** decide who the primary
governing representatives are. They are shown with Authority, and left out
wherever they do not apply.

**Representation** is either a single representative or a group:

| Authority                                                     | Representation                      |
| ------------------------------------------------------------- | ----------------------------------- |
| Local Council, Elders / Community Leaders, Popular Assembly   | Always a group                      |
| Noble / Landed Ruler, Appointed Official, Religious Authority | Always single (1 representative)    |
| Merchant / Guild Rule, Military Authority                     | Roll **1d4**: 1–2 single, 3–4 group |

- The always-single and always-group Authority tags emit
  `authorityRepresentation.single` or `authorityRepresentation.group`
  themselves (an `add_tag` effect, editable in the Tag editor).
- `table.authorityRepresentation` (1d4, results `single` and `group`) is only
  eligible while neither tag is active, so it is rolled for Guild and Military
  rule only, and for any new Authority result that does not decide it.

**Authority Size** (`table.authoritySize`) is only eligible while
`authorityRepresentation.group` is active. It is rolled on **1d10**, with an
equal 20% chance of each size:

| 1d10 | Result            | Value |
| ---- | ----------------- | ----- |
| 1–2  | 2 Representatives | 2     |
| 3–4  | 3 Representatives | 3     |
| 5–6  | 4 Representatives | 4     |
| 7–8  | 5 Representatives | 5     |
| 9–10 | 6 Representatives | 6     |

- Authority Size is the number of **primary** representatives who will later
  receive Authority Figure NPCs, not the membership of the wider body. A
  Popular Assembly may be a broad civic body with, say, four representatives.
- It is structured data (each result's `value`), not a tag.
- A single representative has an Authority Size of 1, and no Size roll.
- Every roll (d12 Authority, d4 Representation, d10 Authority Size) is
  recorded. The saved snapshot also stores the outcome as
  `{ authority, tags, representation, size }`.
- God Mode cannot make Authority Size apply to a single ruler: it lifts result
  eligibility only, never a table's eligibility.

### Authority Figures

The **Authority Figures** stage gives a rough, roleplay-ready outline of the
people who hold the Authority. It is deliberately limited: no appearance,
stat block, alignment, biography or secrets.

- **Input.** The `AuthorityRepresentatives` contract (`authority.rs`): the
  Authority, its representation and its size, read from the settlement so
  far. A single representative gives one figure; a group gives exactly
  Authority Size figures. Each figure is meant to receive a **Role / Title**
  resolved beforehand. No Role / Title stage exists yet, so figures currently
  have none.
- **Fields.** Every figure rolls each field on its own physical-dice table,
  in this order:

  | Field               | Table                               | Dice |
  | ------------------- | ----------------------------------- | ---- |
  | Species             | `table.authorityFigure.species`     | 1d20 |
  | Gender              | `table.authorityFigure.gender`      | 1d20 |
  | Age                 | `table.authorityFigure.age`         | 1d12 |
  | Disposition         | `table.authorityFigure.disposition` | 1d12 |
  | Motive              | `table.authorityFigure.motive`      | 1d12 |
  | Reputation          | `table.authorityFigure.reputation`  | 1d12 |
  | Flaw / Complication | `table.authorityFigure.flaw`        | 1d12 |

- **Species** follows the Environment pattern. `table.authorityFigure.species`
  is a stage table that `world.kurovia` redirects to the **temporary**
  `table.authorityFigure.species.kurovia` (1–4 Human, 5–6 High Elf, 7–8 Wood
  Elf, 9 Dark Elf, 10 Dragonborn, 11 Earth Genasi, 12 Air Genasi, 13 Fire
  Genasi, 14 Water Genasi, 15 Yuan-Ti, 16–17 Dwarf, 18 Eladrin, 19–20
  Firbolg). Each result emits a `species.*` tag. The list and its weights are
  expected to change and are editable like any table.
- **Gender** (1–9 Man, 10–18 Woman, 19–20 Non-binary) emits `gender.man`,
  `gender.woman` or `gender.nonBinary`. **Age** is a broad life stage (Young
  Adult, Adult, Middle-aged, Older, Elderly), not a number of years.
  **Disposition** is the figure's default manner; **Reputation** is how the
  settlement sees them; a **Flaw / Complication** is not an alignment.
- **Figure tags.** A figure's `species.*` and `gender.*` tags belong to the
  figure. They route its name and never become settlement tags.
- **Names** route by the figure's species (and gender, where the species
  uses it) through species-specific **person** naming schemes: original,
  temporary name tables for all thirteen species (see
  [NAMING_SYSTEM.md](NAMING_SYSTEM.md)). There is no universal name table. A
  species without one (e.g. added by a user) shows "No name table configured
  for <species>.", is called "Figure n", and can be named by hand.
- **Relationships.** With two or more figures, every **unordered pair** gets
  exactly one relationship on `table.authorityRelationship` (1d10: 1 Hates, 2
  Despises, 3 Distrusts, 4 Dislikes, 5–6 Neutral, 7 Respects, 8 Likes, 9
  Trusts, 10 Admires). `n` figures have `n(n−1)/2` relationships, recorded
  as `A ↔ B` by stable NPC ID, never once in each direction.
- **Manual modes.** Each field, name and relationship can be chosen (legal
  results only; God Mode lifts result eligibility, never a table's routing),
  rerolled, or, for names, typed. Changing the Authority or its size
  invalidates the figures. Changing a figure's species or gender routes its
  name afresh, unless the name was typed.
- **Saved settlements** keep every figure (stable `npc-…` ID, label, role,
  each field's full roll record, name and its rolls, the figure's tags,
  application version) and every relationship (both NPC IDs, result, d10
  roll). Later content changes never alter them. Settlements saved before
  Authority Figures existed show none, and none are generated unless the
  user asks.

Each Authority result also carries a reserved `generator` reference
(`authorityNpcs.<result id>`) for a future, Authority-specific generator.
Nothing resolves it yet.

## Pending modifiers (planned intents)

Every known modifier is stored as a **planned `intent` effect** on the tag that
causes it. Each intent records the target table and results, or a free-text
concept when the destination table does not exist yet, plus the direction and
a note. An intent on an ordered table with no results (for example
`table.economy`, `increase`) means "shift along the scale" in that direction.
The arrows below are directions only: ↑↑ and ↓↓ are `strong_increase` and
`strong_decrease`, and **the mechanical strength of every intent is
unresolved**. The generator never executes intents, and validation rejects an intent
marked active. No numeric dice changes are invented.

| Source tag                      | Intent                                                                                            |
| ------------------------------- | ------------------------------------------------------------------------------------------------- |
| `env.farmlandOpenCountry`       | Purpose: Agriculture ↑                                                                            |
| `env.woodland`                  | Purpose: Forestry ↑                                                                               |
| `env.coast`                     | Purpose: Fishing ↑, Trade ↑, Transport / Waystation ↑                                             |
| `env.mountains`, `env.badlands` | Purpose: Resource Extraction ↑                                                                    |
| `env.ichorFields`               | Purpose: Resource Extraction ↑↑                                                                   |
| `env.crystallineWilds`          | none (stated in its description)                                                                  |
| `connection.majorRoute`         | Purpose: Trade ↑, Transport ↑; Economy ↑                                                          |
| `connection.isolated`           | Purpose: Trade ↓, Transport ↓; Economy ↓                                                          |
| `purpose.trade`                 | Economy ↑; Authority: Merchant / Guild Rule ↑                                                     |
| `purpose.military`              | Authority: Military Authority ↑                                                                   |
| `purpose.religious`             | Authority: Religious Authority ↑                                                                  |
| `purpose.administrative`        | Authority: Local Council ↑, Appointed Official ↑                                                  |
| `size.tradingPost`              | Purpose: Trade ↑↑, Transport ↑; Authority: Guild Rule ↑                                           |
| `size.capital`                  | Purpose: Administrative ↑↑; Authority: see below                                                  |
| `origin.strategicPosition`      | security-related rolls ↑ (no table yet)                                                           |
| `origin.militaryOutpost`        | security-related rolls ↑; Purpose: Military ↑; Authority: Military Authority ↑                    |
| `origin.tradeStop`              | shops / commercial buildings ↑; Purpose: Trade ↑, Transport ↑; Authority: Merchant / Guild Rule ↑ |
| `origin.resourceDiscovery`      | unique resource (guarantee); Purpose: Resource Extraction ↑                                       |
| `origin.refugeResettlement`     | population-related rolls ↑                                                                        |
| `origin.religiousFoundation`    | religious buildings / temples ↑; Purpose: Religious ↑; Authority: Religious Authority ↑           |
| `origin.estateSeat`             | at least one noble (guarantee); Authority: Noble / Landed Ruler ↑                                 |
| `age.new`                       | Size ↓ (smaller), population ↓, opportunities ↑                                                   |
| `age.old`, `age.ancient`        | Size ↑ (larger), population ↑                                                                     |
| `economy.failing`               | opportunities ↑↑, building quality ↓↓, economic pressures ↑                                       |
| `economy.struggling`            | opportunities ↑, building quality ↓                                                               |
| `economy.stable`                | none                                                                                              |
| `economy.prosperous`            | shops / services ↑                                                                                |
| `economy.wealthy`               | shops / services ↑↑, high-quality / notable buildings ↑                                           |

`size.capital` on Authority: Appointed Official ↑, Noble / Landed Ruler ↑,
Local Council ↑, Elders / Community Leaders ↓. On Authority, ↑ means "more
likely" and ↓ "less likely"; how much more or less likely is not defined.

The Opportunities, Building Quality, Economic Pressures, Shops / Services and
Notable Buildings systems do not exist yet. Their intents use a free-text
`concept` and can be pointed at the real tables once those are added.

## Incomplete content (release blockers)

| Item                            | Missing                                           |
| ------------------------------- | ------------------------------------------------- |
| `table.environment`             | Environment content for non-Kurovia regions       |
| `table.horrorRegion`            | Not designed (planned redirect only)              |
| `table.authorityFigure.species` | Authority Figure species for non-Kurovia settings |

Validation reports incomplete tables with the `incomplete_content` code. The
bundled-content test pins this list, so finishing or adding an incomplete item
is always a deliberate change.

## Out of scope for v0.1.0

The following are not included: Location, Development, Materials, Broad
Character, Fortress content and NPC generation.
