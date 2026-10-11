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

| #   | Stage           | Table                                                    |
| --- | --------------- | -------------------------------------------------------- |
| 1   | Setting Profile | `table.settingProfile` (chosen)                          |
| 2   | Region          | `table.genericRegion`, redirected by region-routing tags |
| 3   | Environment     | `table.environment`, redirected by Kurovia region tags   |
| 4   | Connections     | `table.connections`                                      |
| 5   | Size            | `table.size` (**incomplete**)                            |
| 6   | Origin          | `table.origin`                                           |
| 7   | Purpose         | `table.purpose`                                          |
| 8   | Age             | `table.age`                                              |

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

Size is **incomplete**. Its result vocabulary is Trading Post, Village, Town,
City and Capital. No dice distribution has been approved, so the table has no
dice and no ranges. Fortress is deliberately deferred to future DLC/content.

## Origin, Purpose, Age

- These are entered as specified: Origin and Purpose use 1d20, and Age uses
  1d8.
- Purpose-Built's description records that the origin equals the Purpose
  generated later.
- The Development table has not been reintroduced.

## Pending modifiers (planned intents)

Every known modifier is stored as a **planned `intent` effect** on the tag that
causes it. Each intent records the target table and results, or a free-text
concept when the destination table does not exist yet, plus the direction and
a note. The generator never executes intents, and validation rejects an intent
marked active. No numeric dice changes are invented.

| Source tag                      | Intent                                                        |
| ------------------------------- | ------------------------------------------------------------- |
| `env.farmlandOpenCountry`       | Purpose: Agriculture ↑                                        |
| `env.woodland`                  | Purpose: Forestry ↑                                           |
| `env.coast`                     | Purpose: Fishing ↑, Trade ↑, Transport / Waystation ↑         |
| `env.mountains`, `env.badlands` | Purpose: Resource Extraction ↑                                |
| `env.ichorFields`               | Purpose: Resource Extraction ↑↑                               |
| `env.crystallineWilds`          | none (stated in its description)                              |
| `connection.majorRoute`         | Purpose: Trade ↑, Transport ↑                                 |
| `connection.isolated`           | Purpose: Trade ↓, Transport ↓                                 |
| `size.tradingPost`              | Purpose: Trade ↑↑, Transport ↑                                |
| `size.capital`                  | Purpose: Administrative ↑↑                                    |
| `origin.strategicPosition`      | security-related rolls ↑ (no table yet)                       |
| `origin.militaryOutpost`        | security-related rolls ↑; Purpose: Military ↑                 |
| `origin.tradeStop`              | shops / commercial buildings ↑; Purpose: Trade ↑, Transport ↑ |
| `origin.resourceDiscovery`      | unique resource (guarantee); Purpose: Resource Extraction ↑   |
| `origin.refugeResettlement`     | population-related rolls ↑                                    |
| `origin.religiousFoundation`    | religious buildings / temples ↑; Purpose: Religious ↑         |
| `origin.estateSeat`             | at least one noble (guarantee)                                |
| `age.new`                       | Size ↓, population ↓, opportunities ↑                         |
| `age.old`, `age.ancient`        | Size ↑, population ↑                                          |

## Incomplete content (release blockers)

| Item                 | Missing                                     |
| -------------------- | ------------------------------------------- |
| `table.size`         | Dice distribution                           |
| `table.environment`  | Environment content for non-Kurovia regions |
| `table.horrorRegion` | Not designed (planned redirect only)        |

Validation reports incomplete tables with the `incomplete_content` code. The
bundled-content test pins this list, so finishing or adding an incomplete item
is always a deliberate change.

## Out of scope for v0.1.0

The following are not included: Location, Development, Materials, Broad
Character, Fortress content and NPC generation.
