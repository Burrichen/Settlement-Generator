# Nareth - Expanded Naming Pack

**Target release:** Settlement Generator v0.2.0 content draft (not a shipped release)  
**Region routing:** `world.kurovia` AND `region.nareth`  
**Schema:** Existing `schema_version: 1` tables + one Nareth-only `naming-schemes.json` entry  
**Purpose:** Original fictional rainforest/wetland settlement names informed by Quechua toponymy and Riot's Ixtal atmosphere, without copying canon or claiming authentic Quechua grammar.

## Identity / direction

Nareth is a wet river-and-rainforest region, not a generic 'Aztec jungle' or an Andean alpine setting. The Kurovian source material establishes existing settlements **Uru Taal** and **Aruven**, a named district **Low Stone**, river families, flood infrastructure and supernatural ecological hunger. These names and sites are **reserved**, not reproduced by the generator.

A few naming styles coexist naturally:

- Real Quechua geographical **naming mechanisms**: descriptions followed by terms for rivers, springs, lagoons, settlements and bridges.
- Fictional old river-family and community naming roots inherited from the initial Nareth starter content.
- Ordinary floodplain living: houses, clearings, crossings, cultivated plots and landing settlements.
- Rare original names evoking concealed monumental architecture and elemental knowledge, inspired by the **atmosphere** of Ixtal.
- Rare curated complete names, including some two-word forms that echo the *structure* (not the contents) of Uru Taal.

No new fictional root is given a fabricated Quechua translation. Real root glosses in JSON descriptions are contextual and simplified.

## Methods and probability

| d100 | Method | Chance | Generation paths |
|---|---|---:|---:|
| 01-25 | Rivers & Confluences | 25% | 1,200 |
| 26-47 | Forest & Canopy | 22% | 1,200 |
| 48-64 | Wetlands & Floodplains | 17% | 960 |
| 65-75 | Villages & Cultivated Clearings | 11% | 1,000 |
| 76-82 | Trails & River Crossings | 7% | 600 |
| 83-88 | Ancient Stone & Terraces | 6% | 800 |
| 89-92 | Old Geographic Compounds | 4% | 800 |
| 93-95 | Ancestral Settlements | 3% | 480 |
| 96-98 | Elemental & Hidden Places | 3% | 800 |
| 99-100 | Inherited Proper Names | 2% | 100 |
| **Total** | | **100%** | **7,940** |

**6,405 distinct case-insensitive spellings** result from these 7,940 component paths. **1,535 additional paths** duplicate a spelling produced elsewhere. This counts combinatorial possibilities, not equal chances; dice faces weight the actual outcomes.

The first three methods contribute **64%** of all naming outcomes: rainforest rivers (25%) + canopy (22%) + floodplain (17%). This keeps Nareth wet and inhabited. Historical/exotic names are present but not overrepresented.

## Examples that are valid actual generation outputs

**Rivers & Confluences:** `Yanayaku`, `Rumimayu`, `Pukaqucha`, `Lurayapukyu`  
**Forest & Canopy:** `Intimarka`, `Kiswarmallki`, `Yuraqwaylla`, `Tikalaya`  
**Wetlands & Floodplains:** `Nalinqucha`, `Uruyaku`, `Chiripukyu`, `Orayamayu`  
**Villages & Cultivated Clearings:** `Arumarka`, `Rumikancha`, `Yanallaqta`, `Chuntawasi`  
**Trails & River Crossings:** `Keshchaka`, `Tarenpunku`, `Wayratampu`, `Varuchaka`  
**Ancient Stone & Terraces:** `Rumipata`, `Pukakancha`, `Intiwasi`, `Tarenqaqa`  
**Old Geographic Compounds:** `Naroneth`, `Korivai`, `Talamar`, `Narothil`  
**Ancestral Settlements:** `Talarien`, `Namarvai`, `Ivaramar`, `Eshartal`  
**Elemental & Hidden Places:** `Xalunkan`, `Tzarineth`, `Querathal`, `Vixaraven`  
**Inherited Proper Names:** `Yuranei`, `Vara Talen`, `Iru Vasha`, `Mariven`  

Names are examples of possible fictional output, not canonically established settlements. Some combinations use authentic Quechua lexical references; others combine invented Nareth roots, and the combinations have **not** been verified for grammar.

## Files

- `01-method.json` - selection across the 10 methods (physical d100).
- `02-first-element.json`, `03-second-element.json` - existing old geographic component tables, expanded to 40 x 20 while preserving all original entry IDs.
- `04-founder.json`, `05-founder-ending.json` - existing historical-person/lineage tables, expanded to 40 x 12 and kept rare.
- `06-river-first.json`, `07-river-second.json` - water and river settlements.
- `08-forest-first.json`, `09-forest-second.json` - forest and canopy.
- `10-wetland-first.json`, `11-wetland-second.json` - flooded woods, marsh and river edges.
- `12-settlement-first.json`, `13-settlement-second.json` - houses, clearings and settlement formations.
- `14-trail-first.json`, `15-trail-second.json` - bridges, waystations and jungle travel corridors.
- `16-stone-first.json`, `17-stone-second.json` - stone-built and terraced places.
- `18-elemental-first.json`, `19-elemental-second.json` - rare original high-culture elemental sound layer.
- `20-inherited-name.json` - 100 pre-curated complete names, including thirty two-word names.
- `naming-schemes.json` - routing, stable method IDs, component paths and joining rules. **Only the Nareth scheme object is supplied. Merge it into any global manifest; do not overwrite other regions.**
- `PREVIEW.csv` - deterministic samples by method.
- `WEIGHTED_PREVIEW.csv` - weighted representative random output.
- `COMPLETE_CATALOG.csv` - all paths, useful for broad editorial inspection.
- `COLLISIONS.csv` - duplicate output names across paths.
- `EDITORIAL_FLAGS.csv` - possible clumsy joins (editorial warning only; not hidden filters).
- `METHOD_WEIGHTS.csv` - all method probabilities.
- `HISTORICAL_RESEARCH.md` - sources and distinctions between real terms and fictional designs.
- `EDITORIAL_NOTES.md` - next refinement suggestions.
- `CLAUDE_INSTALL_PROMPT.md` - safe integration into the working GUI/CLI application.
- `AUTHORING_PREVIEW.py` - standalone authoring validator and preview tool.
- `VALIDATION.md` - automated checks and known limitations.

## VS Code editing

1. Unzip the pack in a working folder and **back up your existing Nareth content**.
2. Edit the `name` of any table result in VS Code. Keep the `id` stable when renaming existing entries so downstream references and existing user overrides remain functional.
3. You may edit results/ranges, but each `dice` expression must continue to cover every physical die face exactly once. Examples: `1d20` must assign exactly 1-20; `1d100` exactly 1-100.
4. Modify `01-method.json` to change the proportion of Nareth settlements using rivers vs forest vs old/inherited forms.
5. Run `python AUTHORING_PREVIEW.py --samples 20 --export` from the extracted folder. The tool validates all tables and generates new CSVs.
6. A table's `result.id` appears in `naming-schemes.json.methods[].result` for method selection; if you rename a method ID, update the manifest as well. Prefer not changing IDs.
7. Merge Nareth's `naming-schemes.json` **entry only** into a shared world manifest if your application uses one. This archive's manifest is *not* a full global file.
8. Restart/reload the app according to its content-loading rules. Existing custom user overrides may mask your edited bundled JSON. Do not overwrite user data or old saved settlement snapshots.

### Compatibility note

All methods use existing `join.style: compound`. The `inherited` method contains a **single component** whose result may already include a space, e.g. `Vara Talen`; the core must preserve this complete string without trimming away the internal space. If your app's engine doesn't already support one-component methods, implement it **generically once**, not through a Nareth-specific branch. No nonstandard JSON fields or new tag-effect language are required.

### Future optional modifiers (NOT ACTIVE)

- `environment.jungle` might favour forest/canopy.
- `environment.wetlands` might favour floodplain/river.
- `age.ancient` might favour inherited names.
- `origin.estate` might favour lineage names.

These are proposed authoring ideas only. Do not silently change probabilities or add hidden context rules without explicitly rollable mappings.
