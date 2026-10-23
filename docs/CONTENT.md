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
| 14  | ↳ Roles           | `{ "authority_roles": true }` (sub-stage): see below     |
| 15  | Authority Figures | `{ "authority_figures": true }`: see below               |
| 16  | Security          | `table.security`                                         |
| 17  | Crime             | `table.crime`                                            |
| 18  | Stability         | `table.stability`                                        |
| 19  | Notable Locations | `{ "notable_locations": true }`: see below               |

Naming content (v0.1.1) is described in [NAMING_SYSTEM.md](NAMING_SYSTEM.md):

- Naming data so far, in `content/kurovia/naming/`:
  - **Ordon**, the approved content: the Ordon Quality Pass v2, with five
    methods (see [ORDON_NAMING.md](ORDON_NAMING.md));
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

### Authority Role Allocation (Roles)

The **Roles** sub-stage gives every Authority Figure a Role / Title
**before** the figures are generated (Authority → Representation → Authority
Size → Roles → Authority Figures → relationships).

- **Single representative:** one roll on the Authority's single role table.
- **Group:** exactly one roll on its **lead** table, then `Authority Size − 1`
  rolls on its **member** table. There is always exactly one role per
  figure, and the lead is always the first figure. The same table is never
  rolled for everyone, so a council never has four Council Chairs.
- **Unique and repeatable roles.** A member result marked `unique` can be
  held once. Rolling (or choosing) it again does **not** reroll: it resolves
  as the result's `fallback`, which must be repeatable. The record keeps the
  die, the result rolled and the role given, e.g. roll 5 → Treasurer →
  Councillor.
- **Routing** is data: a **role scheme** per Authority type (`roles.<type>`,
  chosen by its `authority.*` tag) names its single, lead and member
  tables. God Mode lifts result eligibility within the active table only;
  it never merges the Authorities' tables.
- **Stable IDs, not tags.** Roles are results with stable IDs (e.g.
  `role.council.treasurer`) but emit no tags: they never join the
  settlement's tag pool.
- **Gendered titles.** One role ID per role. A result's `label_variants`
  give its title by the holder's gender (e.g. `role.noble.lord` shows as
  Lord for `gender.man` and Lady for `gender.woman`). The title is settled
  once the figure's gender is rolled; without a variant (e.g. a non-binary
  figure) the neutral name is shown, such as "Lord / Lady".
- **Changes.** Changing the Authority, its Representation or its size
  invalidates the roles (and the figures). Changing a role keeps the
  figures and retitles them. Settlements saved before roles existed show
  none, and none are allocated unless the user asks.

| Authority (scheme)                   | Single (1d6)                                                                                                 | Lead                                                                        | Member                                                                                                                                                   |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Local Council (`roles.council`)      | —                                                                                                            | 1d6: 1–3 Council Chair, 4–5 First Councillor, 6 Civic Speaker               | 1d6: 1–3 Councillor, 4 Community Representative, 5 Treasurer (unique → Councillor), 6 Civic Steward (unique → Councillor)                                |
| Noble / Landed Ruler (`roles.noble`) | 1–2 Lord / Lady, 3 Baron / Baroness, 4 Count / Countess, 5 Castellan, 6 Landed Noble                         | —                                                                           | —                                                                                                                                                        |
| Appointed Official                   | 1 Governor, 2 Magistrate, 3 Administrator, 4 Steward, 5 Prefect, 6 Commissioner                              | —                                                                           | —                                                                                                                                                        |
| Merchant / Guild Rule                | 1–2 Guildmaster, 3 Trade Master, 4 Merchant Principal, 5 Senior Factor, 6 Guild Representative               | 1d6: 1–3 Guildmaster, 4 Trade Master, 5 Merchant Principal, 6 Senior Factor | 1d6: 1–2 Guild Representative, 3 Senior Merchant, 4 Factor, 5 Treasurer (unique → Guild Representative), 6 Trade Speaker (unique → Guild Representative) |
| Religious Authority                  | 1–2 High Priest / High Priestess, 3 Abbot / Abbess, 4 Prior / Prioress, 5 Temple Keeper, 6 Spiritual Steward | —                                                                           | —                                                                                                                                                        |
| Military Authority                   | 1–2 Commander, 3 Captain, 4 Marshal, 5 Warden, 6 Commandant                                                  | 1d6: 1–3 Commander, 4 Marshal, 5 Commandant, 6 Warden                       | 1d6: 1–2 Captain, 3 Senior Officer, 4 Lieutenant, 5 Quartermaster (unique → Senior Officer), 6 Adjutant (unique → Senior Officer)                        |
| Elders / Community Leaders           | —                                                                                                            | 1d6: 1–4 First Elder, 5–6 Community Speaker                                 | 1d6: 1–4 Elder, 5 Community Representative, 6 Senior Elder (all repeatable)                                                                              |
| Popular Assembly                     | —                                                                                                            | 1d4: 1–2 Speaker, 3 Convenor, 4 Presiding Delegate                          | 1d4: 1–3 Delegate, 4 Citizen Representative (both repeatable)                                                                                            |

Role tables are `table.authorityRole.<type>.<single|lead|member>`
(`content/12-authority-roles/`, one file per table, plus
`role-schemes.json`), editable like any table: dice, faces,
names, the unique flag, the fallback and display variants (the result's
**Role options** in the GUI). Result IDs follow the approved
`role.<type>.<role>` IDs; the same role keeps the same ID across tables
(e.g. `role.guild.guildmaster` on the single and lead tables). Popular
Assembly's Authority Size is its primary representatives, not every
assembly member. No guild, religion, military unit or noble house is
generated.

### Authority Figures

The **Authority Figures** stage gives a rough, roleplay-ready outline of the
people who hold the Authority. It is deliberately limited: no appearance,
stat block, alignment, biography or secrets.

- **Input.** The `AuthorityRepresentatives` contract (`authority.rs`): the
  Authority, its representation and its size, read from the settlement so
  far. A single representative gives one figure; a group gives exactly
  Authority Size figures. Each figure receives the **Role / Title** the
  Roles stage allocated to its position (lead first) before any of its
  details are rolled.
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

## Security, Crime and Stability

Three broad settlement conditions after the Authority, in this order, each
on its own **1d12** table (`content/13-security.json`, `14-crime.json`,
`15-stability.json`). All three are **ordered scales** (`"ordered": true`,
least to most) and their extremes are deliberately rarer. Each result emits
its tag. No guards, criminal organisations, riots or other detailed systems
are generated.

| 1d12 | Security (`security.*`)              | Crime (`crime.*`)     | Stability (`stability.*`) |
| ---- | ------------------------------------ | --------------------- | ------------------------- |
| 1    | Undefended (`undefended`)            | Minimal (`minimal`)   | Volatile (`volatile`)     |
| 2–4  | Lightly Defended (`lightlyDefended`) | Low (`low`)           | Unsettled (`unsettled`)   |
| 5–8  | Guarded (`guarded`)                  | Moderate (`moderate`) | Stable (`stable`)         |
| 9–11 | Well Defended (`wellDefended`)       | High (`high`)         | Cohesive (`cohesive`)     |
| 12   | Fortified (`fortified`)              | Rampant (`rampant`)   | Unified (`unified`)       |

- **Security** is how well protected and defended the settlement is.
- **Crime** is the overall prevalence of criminal activity.
- **Stability** is how socially and politically settled, cohesive and
  resistant to internal unrest the settlement is. It is not Crime, Security
  or the kind of Authority: no Authority makes a settlement more or less
  stable.

Their modifiers are planned intents (see below): directions only. **No
magnitude is approved, and nothing shifts a roll.** Each intent on an
ordered table records a shift along its scale for when one is approved.

- Origin: Strategic Position and Military Outpost raise Security (these
  replace the earlier placeholder "security-related rolls ↑").
- Security → Crime: Undefended ↑↑, Lightly Defended ↑, Guarded none, Well
  Defended ↓, Fortified ↓↓.
- Economy → Crime is deliberately **U-shaped**: poverty breeds crime, and
  wealth offers more valuable targets and opportunities.

  ```
  Failing      → Crime ↑↑
  Struggling   → Crime ↑
  Stable       → Crime neutral
  Prosperous   → Crime ↑
  Wealthy      → Crime ↑↑
  ```

- Economy → Stability is **not** U-shaped; a Wealthy settlement can have
  more crime and still be very stable.

  ```
  Failing      → Stability ↓↓
  Struggling   → Stability ↓
  Stable       → Stability neutral
  Prosperous   → Stability ↑
  Wealthy      → Stability ↑
  ```

- Crime → Stability: Minimal ↑, Low ↑, Moderate none, High ↓, Rampant ↓↓.
- High and Rampant Crime raise future pressures, opportunities, criminal
  organisations / factions, demand for guards / security, crime-related
  noticeboard jobs and crime-related buildings / services. Lower Crime may
  reduce these later; that is not yet decided.
- Stability → Opportunities, for the future Opportunity system (not
  implemented):

  ```
  Volatile  → Opportunities ↑↑
  Unsettled → Opportunities ↑
  Stable    → neutral
  ```

  Cohesive and Unified give no bonus Opportunities.

- Future Stability hooks (pressures, unrest, protests, riots, political
  complications, factional conflict, migration and population, noticeboard
  content) are noted here only; no intent records them until their direction
  per result is approved.

## Notable Locations

`content/16-notable-locations/`, one table per file. A **Notable Location** is
a place worth a name on the settlement sheet: somewhere a party might visit,
ask about or be sent to. Ordinary buildings (homes, sheds, unremarkable
workshops) are not Notable Locations and are not generated.

**Count by Size.** The scheme in `location-schemes.json` that matches the
settlement's Size tag picks the count table. Each count result carries its
number as `value`.

| Size             | Table                                        | Die  | Count                              |
| ---------------- | -------------------------------------------- | ---- | ---------------------------------- |
| Trading Post     | `table.notableLocationCount.tradingPost`     | 1d4  | 1–2: 1, 3–4: 2                     |
| Village          | `table.notableLocationCount.village`         | 1d6  | 1–2: 1, 3–4: 2, 5–6: 3             |
| Town             | `table.notableLocationCount.town`            | 1d6  | 1–2: 2, 3–4: 3, 5–6: 4             |
| City district    | `table.notableLocationCount.cityDistrict`    | 1d8  | 1–2: 2, 3–4: 3, 5–6: 4, 7–8: 5     |
| Capital district | `table.notableLocationCount.capitalDistrict` | 1d10 | 2 / 3 / 4 / 5 / 6 (two faces each) |

**Cities and Capitals are mapped by district.** The District Count is rolled
first, then each district rolls its own Notable Location Count:

| Table                         | Die | Districts              |
| ----------------------------- | --- | ---------------------- |
| `table.districtCount.city`    | 1d6 | 1–2: 2, 3–4: 3, 5–6: 4 |
| `table.districtCount.capital` | 1d6 | 1–2: 3, 3–4: 4, 5–6: 5 |

So a City has **2–4** districts and a Capital **3–5**. Trading Posts,
Villages and Towns never roll a District Count: they roll one count for the
whole settlement. Each district is a stable generation and map container with
an ID (`district-1`, …) that does not depend on its label (District 1, …).
District Types and placing districts on a map are future work.

**Notable Location Type** (`table.notableLocationType`, a genuine 1d20), rolled
once per rolled location. Result IDs are stable:

| d20   | Type                  | ID                           |
| ----- | --------------------- | ---------------------------- |
| 1–3   | Hospitality           | `locationType.hospitality`   |
| 4–6   | Trade / Shop          | `locationType.trade`         |
| 7–8   | Craft / Workshop      | `locationType.craft`         |
| 9–10  | Religious             | `locationType.religious`     |
| 11    | Civic / Government    | `locationType.civic`         |
| 12    | Military / Security   | `locationType.military`      |
| 13    | Guild / Organisation  | `locationType.organisation`  |
| 14    | Knowledge / Education | `locationType.knowledge`     |
| 15    | Entertainment         | `locationType.entertainment` |
| 16    | Criminal              | `locationType.criminal`      |
| 17–18 | Industry / Resource   | `locationType.industry`      |
| 19–20 | Unusual / Special     | `locationType.unusual`       |

**Criminal replaces Residence / Estate.** There is no Residence / Estate
result: noble residences and estates belong to the future **Noble system**,
which will generate them itself. The Type emits no settlement tags.

### Subtypes: one canonical ID per subtype

Hospitality, Trade / Shop and Craft / Workshop roll a **subtype** straight
after the Type; the other Types have no subtype table yet.
`location-subtypes.json` holds:

- `location_subtype_tables`: the subtype table each Type rolls;
- `location_subtypes`: the **canonical subtypes**, each listed **once**
  however many tables use it.

**The rule.** A Notable Location subtype has **one stable ID across the whole
Notable Locations system**, even when several parent Type tables list it. A
subtype table lists a subtype as a result whose ID _is_ the canonical ID. The
result's name is only the subtype's **label in that table**, and its faces
are its weight there. There are never per-table copies such as
`shop.weaponsmith` and `workshop.weaponsmith`. The generated location keeps
its parent Type separately:

| Type                 | Subtype                         | Label at generation       |
| -------------------- | ------------------------------- | ------------------------- |
| `locationType.trade` | `locationSubtype.generalSmithy` | General Smithy            |
| `locationType.craft` | `locationSubtype.generalSmithy` | General Smithy            |
| `locationType.trade` | `locationSubtype.jeweller`      | Jeweller / Precious Goods |
| `locationType.craft` | `locationSubtype.jeweller`      | Jeweller / Goldsmith      |

The rule applies to every current and future Notable Location table, and
validation enforces it:

- every result of a subtype table is a canonical subtype
  (`unknown_location_subtype`); a result added in the table editor is a
  custom subtype of its own table;
- no two canonical subtypes share a name (`duplicate_subtype_name`);
- an added result named like a canonical subtype is flagged
  (`looks_like_location_subtype`), because it would be a separate subtype.

Renaming a result in the table editor only changes its label in that table:
result IDs cannot be edited. The editor lists each table's canonical subtypes
and the other tables that use them.

Some results need more dice before the subtype is final. This is data on the
canonical subtype (`resolve`):

- `split`: roll another table; its result is the subtype (Weaponsmith /
  Armoursmith);
- `reroll`: roll the same table again, as often as the result comes up, and
  give the location a future Quality shift **once** (Specialist Artisan).

A subtype with a `speciality` table rolls it once. The result is child data of
the location (a Magic Store's speciality), never a Notable Location of its
own. Every roll is recorded with the location.

**Hospitality** (`table.notableLocationSubtype.hospitality`, 1d10):

| d10  | Subtype      | ID                            |
| ---- | ------------ | ----------------------------- |
| 1–6  | Tavern / Inn | `locationSubtype.tavernInn`   |
| 7–8  | Eating House | `locationSubtype.eatingHouse` |
| 9–10 | Bathhouse    | `locationSubtype.bathhouse`   |

Tavern and Inn are deliberately **one** subtype.

**Trade / Shop** (`table.notableLocationSubtype.trade`, 1d20) is the
specialist, gameplay-relevant places where players are likely to buy useful
goods. It does not try to represent all commerce in the settlement: a
**General Store** is baseline functionality of every settlement, and
**Markets** will be a separate future roll, so neither is on it.

| d20   | Subtype                   | ID                                    |
| ----- | ------------------------- | ------------------------------------- |
| 1–8   | General Smithy            | `locationSubtype.generalSmithy`       |
| 9–13  | Alchemist / Apothecary    | `locationSubtype.alchemistApothecary` |
| 14–16 | Magic Store               | `locationSubtype.magicStore`          |
| 17–18 | Bowyer / Fletcher         | `locationSubtype.bowyerFletcher`      |
| 19    | Rare Goods / Curiosities  | `locationSubtype.rareGoods`           |
| 20    | Jeweller / Precious Goods | `locationSubtype.jeweller`            |

- **General Smithy** is useful mundane metalwork: basic weapons and armour,
  tools, repairs, metal fittings and ordinary smithing. It is not a
  specialist Weaponsmith or Armoursmith.
- **Alchemist / Apothecary** stays one combined subtype.

**Magic Store Speciality** (`table.magicStoreSpeciality`, 1d6) is rolled once
for every Magic Store, and only for a Magic Store. A Magic Store is a
specialist dealer, not a store of every kind of magic item.

| d6  | Speciality                   | ID                                         |
| --- | ---------------------------- | ------------------------------------------ |
| 1   | Armour & Protection          | `magicStoreSpeciality.armourProtection`    |
| 2   | Books, Scrolls & Lore        | `magicStoreSpeciality.booksScrollsLore`    |
| 3   | Clothing                     | `magicStoreSpeciality.clothing`            |
| 4   | Jewellery                    | `magicStoreSpeciality.jewellery`           |
| 5   | Weapons                      | `magicStoreSpeciality.weapons`             |
| 6   | Curiosities & Wondrous Items | `magicStoreSpeciality.curiositiesWondrous` |

**Craft / Workshop** (`table.notableLocationSubtype.craft`, 1d20) is places
focused on making, repairing, improving, commissioning and specialised
production. It is distinct from Trade / Shop even where they share a subtype.

| d20   | Subtype                   | ID                                                    |
| ----- | ------------------------- | ----------------------------------------------------- |
| 1–4   | General Smithy            | `locationSubtype.generalSmithy`                       |
| 5–8   | Weaponsmith / Armoursmith | `locationSubtype.weaponsmithArmoursmith` (split, 1d4) |
| 9–12  | Builder / Mason           | `locationSubtype.builderMason`                        |
| 13–14 | Bowyer / Fletcher         | `locationSubtype.bowyerFletcher`                      |
| 15–16 | Alchemist / Apothecary    | `locationSubtype.alchemistApothecary`                 |
| 17    | Engineer / Mechanist      | `locationSubtype.engineerMechanist`                   |
| 18    | Tailor / Weaver           | `locationSubtype.tailorWeaver`                        |
| 19    | Jeweller / Goldsmith      | `locationSubtype.jeweller`                            |
| 20    | Specialist Artisan        | `locationSubtype.specialistArtisan` (reroll)          |

**Weaponsmith / Armoursmith**
(`table.notableLocationSubtype.weaponsmithArmoursmith`, 1d4) is an exact
50/50 split: 1–2 **Weaponsmith** (`locationSubtype.weaponsmith`), 3–4
**Armoursmith** (`locationSubtype.armoursmith`). The location keeps both the
Craft / Workshop roll and the d4.

**Builder / Mason and Engineer / Mechanist** carry planned capabilities for
the future player Keep upgrades. They are metadata only, copied to the
location when it is generated; the Keep upgrade system does not exist yet.

| Subtype              | Capability               | For                                                                                                                                        |
| -------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Builder / Mason      | `keepUpgrade.structural` | Structural expansion, rooms or wings, walls, towers, gates, fortification, courtyards, major repairs                                       |
| Engineer / Mechanist | `keepUpgrade.mechanical` | Defensive mechanisms, hoists, lifts, reinforced gates, machinery, mechanical traps, complex fortification improvements, special facilities |

**Specialist Artisan** (Craft / Workshop 20) is not a final subtype. When it
comes up:

1. the original 20 is recorded;
2. Craft / Workshop is rolled again for what the artisan makes, and again on
   every further 20, each roll recorded;
3. the first non-20 result is resolved normally (5–8 still rolls the d4);
4. the location gets **one** future Quality +1 step (`quality_shift`), however
   many 20s came up.

Example: 20 → Specialist Artisan; reroll 13 → Bowyer / Fletcher. The subtype
is Bowyer / Fletcher, with Quality +1 step.

### Future Quality scale

The approved Quality vocabulary, lowest first (`quality.rs`):

| Quality       | ID                     |
| ------------- | ---------------------- |
| Poor          | `quality.poor`         |
| Below Average | `quality.belowAverage` |
| Average       | `quality.average`      |
| Above Average | `quality.aboveAverage` |
| Excellent     | `quality.excellent`    |

**The Quality generation table is still pending.** No distribution has been
designed, so none exists, and no location is given a Quality. Only a pending
shift is recorded, such as the Specialist Artisan's +1 step. Once Quality is
generated, a shift moves up the scale and stops at Excellent: there is
nothing above it.

### Bare Minimum Buildings

**Bare Minimum Buildings** (a setting, off by default) gives every settlement
exactly **one** extra Notable Location: Type **Hospitality**, subtype
**Tavern / Inn** (`locationSubtype.tavernInn`), with **exactly 4 spare
rooms**, source `bareMinimumBuildings`.

- It is **additive**: it never uses up a rolled location. A Trading Post that
  rolls 1 has two Notable Locations with the setting on.
- It is **one per settlement**, generated once and never per district, and
  belongs to no district.
- Neither the Type table nor the Hospitality table is rolled for it: the
  subtype is assigned. It cannot be re-rolled.
- It is listed first and marked with its source, so a rolled Tavern / Inn is
  never mistaken for it.
- Turning the setting off during generation removes only that Tavern / Inn;
  the rolled locations stay. A saved settlement keeps what it was saved with.

A rolled Tavern / Inn will later roll its own number of rooms; that table
does not exist yet.

### Manual and automatic generation

Automatic generation follows the whole tree with physical dice: Type, subtype,
then only what that subtype needs (reroll, split, speciality). In Manual
generation each rolled location offers its Type, its subtype and only the
child rolls that apply to it: a Magic Store Speciality for a Magic Store, the
d4 for a Weaponsmith / Armoursmith, and the reroll for a Specialist Artisan.
The reroll's choices never include Specialist Artisan itself. Changing a roll
re-rolls everything that follows from it.

**Future work.** Subtypes for the other Types (Religious, Civic, and so on),
District Types, Market generation, General Store generation, the room count
of rolled Tavern / Inns, Keep upgrades, Quality generation, shop stock and
inventories, criminal organisations, the Noble system, location NPCs,
interiors, map placement and settlement layout are not generated yet.

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

| Source tag                                  | Intent                                                                                            |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `env.farmlandOpenCountry`                   | Purpose: Agriculture ↑                                                                            |
| `env.woodland`                              | Purpose: Forestry ↑                                                                               |
| `env.coast`                                 | Purpose: Fishing ↑, Trade ↑, Transport / Waystation ↑                                             |
| `env.mountains`, `env.badlands`             | Purpose: Resource Extraction ↑                                                                    |
| `env.ichorFields`                           | Purpose: Resource Extraction ↑↑                                                                   |
| `env.crystallineWilds`                      | none (stated in its description)                                                                  |
| `connection.majorRoute`                     | Purpose: Trade ↑, Transport ↑; Economy ↑                                                          |
| `connection.isolated`                       | Purpose: Trade ↓, Transport ↓; Economy ↓                                                          |
| `purpose.trade`                             | Economy ↑; Authority: Merchant / Guild Rule ↑                                                     |
| `purpose.military`                          | Authority: Military Authority ↑                                                                   |
| `purpose.religious`                         | Authority: Religious Authority ↑                                                                  |
| `purpose.administrative`                    | Authority: Local Council ↑, Appointed Official ↑                                                  |
| `size.tradingPost`                          | Purpose: Trade ↑↑, Transport ↑; Authority: Guild Rule ↑                                           |
| `size.capital`                              | Purpose: Administrative ↑↑; Authority: see below                                                  |
| `origin.strategicPosition`                  | Security ↑                                                                                        |
| `origin.militaryOutpost`                    | Security ↑; Purpose: Military ↑; Authority: Military Authority ↑                                  |
| `origin.tradeStop`                          | shops / commercial buildings ↑; Purpose: Trade ↑, Transport ↑; Authority: Merchant / Guild Rule ↑ |
| `origin.resourceDiscovery`                  | unique resource (guarantee); Purpose: Resource Extraction ↑                                       |
| `origin.refugeResettlement`                 | population-related rolls ↑                                                                        |
| `origin.religiousFoundation`                | religious buildings / temples ↑; Purpose: Religious ↑; Authority: Religious Authority ↑           |
| `origin.estateSeat`                         | at least one noble (guarantee); Authority: Noble / Landed Ruler ↑                                 |
| `age.new`                                   | Size ↓ (smaller), population ↓, opportunities ↑                                                   |
| `age.old`, `age.ancient`                    | Size ↑ (larger), population ↑                                                                     |
| `economy.failing`                           | opportunities ↑↑, building quality ↓↓, economic pressures ↑; Crime ↑↑; Stability ↓↓               |
| `economy.struggling`                        | opportunities ↑, building quality ↓; Crime ↑; Stability ↓                                         |
| `economy.stable`                            | none                                                                                              |
| `economy.prosperous`                        | shops / services ↑; Crime ↑; Stability ↑                                                          |
| `economy.wealthy`                           | shops / services ↑↑, high-quality / notable buildings ↑; Crime ↑↑; Stability ↑                    |
| `security.*`                                | Crime: see Security, Crime and Stability                                                          |
| `crime.*`                                   | Stability; future systems: see Security, Crime and Stability                                      |
| `stability.volatile`, `stability.unsettled` | Opportunities ↑↑ / ↑                                                                              |
| Notable Location Type, see below            | `locationType.*` ↑ (one result, or several)                                                       |

`size.capital` on Authority: Appointed Official ↑, Noble / Landed Ruler ↑,
Local Council ↑, Elders / Community Leaders ↓. On Authority, ↑ means "more
likely" and ↓ "less likely"; how much more or less likely is not defined.

Notable Location Type intents (all ↑ unless stated):

| Source tag                                    | Type                                  |
| --------------------------------------------- | ------------------------------------- |
| `purpose.trade`                               | Trade / Shop                          |
| `purpose.craftIndustry`                       | Craft / Workshop, Industry / Resource |
| `purpose.resourceExtraction`                  | Industry / Resource                   |
| `purpose.fishing`                             | Trade / Shop, Industry / Resource     |
| `purpose.forestry`                            | Craft / Workshop, Industry / Resource |
| `purpose.religious`, `authority.religious`    | Religious                             |
| `purpose.military`, `authority.military`      | Military / Security                   |
| `purpose.administrative`                      | Civic / Government                    |
| `purpose.scholarly`                           | Knowledge / Education                 |
| `authority.guild`                             | Guild / Organisation                  |
| `economy.prosperous`, `economy.wealthy`       | Trade / Shop                          |
| `security.wellDefended`, `security.fortified` | Military / Security                   |
| `crime.high`                                  | Criminal                              |
| `crime.rampant`                               | Criminal ↑↑                           |

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
