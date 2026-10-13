# Halingo - Expanded Naming Pack (editorial draft)

## What this is

An authorable naming pack modelled on your Bratislor `schema_version: 1` files and the original Halingo naming prompt. It is **NOT a tested byte-for-byte replacement for existing Halingo files**, which were not attached. See integration notes below.

The package intentionally preserves the five *expected* original Halingo table IDs:
- `table.naming.halingo.method`
- `table.naming.halingo.firstElement`
- `table.naming.halingo.secondElement`
- `table.naming.halingo.founder`
- `table.naming.halingo.founderEnding`

And it preserves the initial twelve geographic roots, twelve site endings, twelve founder names, and six founder site endings from the earlier project prompt.

**Established Halingo canon:** the region's black Ichor, Ichor wells, railways, Diesel Barons and the place name **Heresy**. The existing settlement name Heresy is **reserved** and intentionally omitted from the random results, so the generator will not accidentally duplicate it. Other generated names here are fictional suggestions, not existing world canon.

## Name count

| Method | Weight | Components | Raw outputs | Joining |
|---|---:|---|---:|---|
| Geographic & Landmark | 15% | 50 x 30 | 1,500 | space |
| Water & Survival | 12% | 30 x 20 | 600 | space |
| Prospecting & Industry | 10% | 40 x 20 | 800 | space |
| Roads, Rails & Commerce | 6% | 30 x 20 | 600 | space |
| Fused Frontier Name | 26% | 100 curated | 100 | 1 component |
| Landholder & Claim | 3% | 40 x 12 | 480 | space |
| Inherited Desert Name | 24% | 50 | 50 | 1 component |
| Disaster & Warning | 2% | 20 x 12 | 240 | space |
| Single-Word Name | 2% | 25 | 25 | 1 component |
| **Total** | **100%** | | **4,395** | |

**Current priority:** Fused Frontier (26%) and Inherited Desert (24%) together account for **50%** of Halingo names. Their 100 and 50 curated entries will therefore recur more frequently than individual geographic cross-product results; expand those vocabularies if repetition becomes noticeable.

Distinct case-insensitive name spellings before authoring changes: **4,060**. The remaining **335** paths duplicate a spelling produced by another method (or another component pairing). Overlaps are not necessarily errors; review `COMPLETE_CATALOG.csv` before culling anything.

Single-word name method has **2%** chance. Its 25 entries each occupy 4 faces of its own `1d100`, so each individual single-word entry (such as Malice or Mercy) has **0.08%** unconditional probability: one in 1,250 generations if no tag modifier is added.

## Editable files

- `01-method.json`: method probabilities; `1d100`.
- `02-first-element.json`: geographic root; 50 x 2 faces of `1d100`.
- `03-second-element.json`: geographic endpoint; 30 weighted results over `1d100`.
- `04-founder.json`: proprietary/stakeholder names; 40 weighted results over `1d100`.
- `05-founder-ending.json`: stakeholder places; 12 results of `1d12`.
- `06-water-first.json` and `07-water-second.json`: water and shelter.
- `08-industry-first.json` and `09-industry-second.json`: mines, Ichor and processing.
- `10-route-first.json` and `11-route-second.json`: railway and waypoints.
- `12-inherited-name.json`: 50 manually curated older place names; single table, no combination.
- `16-fused-boomtown.json`: 100 hand-curated historical-style fused names, e.g. `Cinderford` and `Ichorfield`.
- `13-warning-first.json` and `14-warning-second.json`: remembered dangers.
- `15-single-word.json`: 25 abstract/ordinary words, including Malice and Mercy; Heresy is reserved.
- `naming-schemes.json`: **HALINGO-ONLY** scheme entry and method/component wiring.
- `PREVIEW.csv`: 20 independently sampled results per method.
- `COMPLETE_CATALOG.csv`: all 4,395 combinatorial paths for editorial review.
- `HISTORICAL_RESEARCH.md`: sources and naming logic.
- `VALIDATION.md`: validation results and integration checks.

## DO NOT overwrite a shared global naming-schemes.json

This archive contains a one-scheme `naming-schemes.json` representing Halingo only. **If the application stores all eight regional schemes in a single global file, merge or replace only the `naming.kurovia.halingo` object, keeping all other regions intact.** If schemes live in region-specific content packs, place this file in the Halingo content folder following existing loader conventions.

## Potential engine compatibility requirements

The supplied Bratislor schema proves `join.style = compound` works for pairs. The original Halingo prompt uses two-word names, but its loader code was not available to inspect. This pack uses `join.style = space` for two-word names and `join.style = compound` for single-component methods. **Confirm the engine recognises `space` and one-component methods; if not, adapt the shared engine to support those two generic behaviours.** Do not revert to concatenated names or hardcode a Halingo exception.

No extra proprietary joining operators are assumed. The old 'Historical Founder' slot is renamed to 'Landholder & Claim' and deliberately uses bare owner + site, e.g. `Varesh Station`. To make a possessive style, add a supported generic possessive join operation later; do not add undocumented apostrophes.

## VS Code editing

1. Locate your existing Halingo naming folder and back it up.
2. Preserve the repository's existing file conventions if they differ.
3. Copy the component files into the Halingo folder, and merge only its scheme definition if using a global manifest.
4. Edit `name` while keeping `id` stable. If removing an entry, rebalance `range` so the dice faces have no gaps or overlaps.
5. Change probabilities by editing `01-method.json` (the method face ranges must cover exactly 1 to 100).
6. Read `COMPLETE_CATALOG.csv` to review combinations. It is generated editorial output, not a runtime source file.
7. Restart/reload naming content according to the current app's loader, test GUI + CLI, and verify user overrides aren't masking changes.
8. Run the app's table validator and tests. This authoring pack's basic checks are described in `VALIDATION.md`.

## Future optional tag influences - NOT active yet

- `environment.desert` / `environment.badlands` can weight particular geographic terms.
- `environment.ichorFields` can increase Industry names if you later decide to.
- `connection.majorRoute` can favour Roads/Rails.
- `connection.isolated` can favour Water/Survival or Inherited Desert.
- `origin.resourceDiscovery` can favour Prospecting.
- `origin.tradeStop` can favour Roads/Rails.
- `age.old` / `age.ancient` can increase Inherited Desert names.

These are suggestions only. The JSON currently applies the base 1d100 table without modifiers. Tag rules must be introduced through the existing tag engine and have explicit physical-dice logic before activation.
