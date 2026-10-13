# Ordon naming — quality reviewed edition (v2)

This revision is a targeted editorial correction to the earlier 5-method Ordon pack, inspired by real English place-name development and a restrained Victorian industrial layer.

**Core principle:** a plausible personal name and a plausible suffix DO NOT guarantee a believable settlement name.

## Measured changes

- Same **five naming methods and original method weights (42/20/18/12/8%)**.
- **1,530 deterministic generation paths**; **1497 distinct name spellings**; **33 duplicated spellings**; **33 remaining heuristic editorial review flags**.
- Old pack had 4,650 paths but many arose from unrestricted concatenation; quality matters more than the raw number.
- Geographic: 50 original first elements × 20 approved suffixes, 1d100 + 1d20; a handful of deterministic spelling overrides.
- Founder: 100 founder names retained, but each has **two individual, hand-authored settlement forms**; 1d100 + 1d4 (A: 1–2, B: 3–4). Oswald now yields **Oswalden** or **Oswaldwick**, never Oswaldsworth.
- Industry: 40 original industry source concepts × 4 approved names per concept, 1d100 + 1d4. Facility names like *Foundrywell* no longer appear.
- Transport: 30 original source concepts × 4 approved names, 1d100 + 1d4. No *Market Market*, *Grand Road*, etc.
- Inherited: 50 curated names; replaced six names sounding more like landmarks or facilities.

## Editable files

- `01-method.json`: roll method and weights.
- `02-first-element.json` / `03-second-element.json`: rural compound vocabulary and d20 suffixes.
- `04-founder.json`: 100 founder entries with a pair of **approved_names** each.
- `05-founder-ending.json`: the physical 1d4 selecting A or B. Old arbitrary suffixes archived to `LEGACY_FOUNDER_ENDINGS.json`.
- `06-industry-first.json`: industry source entries, each with four **approved_names**.
- `07-industry-second.json`: d4 to choose A-D.
- `08-route-first.json`: transport source entries, each with four **approved_names**.
- `09-route-second.json`: d4 to choose A-D.
- `10-inherited-name.json`: full preapproved names.
- `naming-schemes.json`: single Ordon scheme only — merge its entry into your existing global registry; **do not replace other region registrations**.

## Important engine requirement

This edition introduces a **generic** `curated_variant` join style for founder, industry and route. For `components=[source_result,variant_result]`, return `source_result.approved_names[variant_result.id]`, rejecting absent names on validation. **Do not silently fall back to concatenation or rejection/rerolls.**

Your current code may need an implementation change. `CLAUDE_INSTALL_PROMPT.md` describes it. This is not represented as a guaranteed drop-in replacement without seeing your app source.

## Manual dice rules

- Roll 1d100 for the method.
- For geographic roll 1d100 prefix and 1d20 suffix, concatenate and apply listed exception if present.
- For founder roll 1d100 source founder and 1d4 form: 1–2 A, 3–4 B; read the exact approved settlement name.
- For industry/route roll 1d100 source word and 1d4 form A–D; read the approved settlement name.
- For inherited roll 1d100 and take the single complete name.

All dice tables are fully covered with no hidden rerolls. The authored forms include selective inherited English possessive -s where it sounds plausible (e.g. Godwinsbury, Nerysbury) rather than mechanically inserting s for every founder and ending.

## QA

`COMPLETE_CATALOG.csv` enumerates all 1,530 possible paths. `COLLISIONS.csv` exposes names that can appear via multiple paths. `EDITORIAL_FLAGS.csv` highlights entries still worth human review. `PREVIEW.csv` shows 140 seeded, method-weighted example generations with dice rolls.

Run `python AUTHORING_PREVIEW.py --samples 20 --export` in this folder after editing.

This is a style-guided fictional name generator, not a claim of historical etymological correctness.
