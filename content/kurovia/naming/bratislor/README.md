# Bratislor expanded naming pack

## Contents

This pack is based directly on the six source files you supplied. It preserves existing table IDs, source result IDs (where previously present), the `schema_version: 1` envelope, `range` conventions, the `naming_schemes` structure, and the existing `compound` join style.

It **does not change your Rust/Tauri implementation**. The three new naming methods are added by extending the existing method table and manifest, assuming your naming engine dynamically supports additional methods as designed. This could not be tested against your actual source code. Back up your original files before replacing them.

| Method | Weight | First table | Second table | Raw combinations |
|---|---:|---:|---:|---:|
| Inherited geographic | 40% | 50 | 25 | 1,250 |
| Family / landholder | 25% | 50 | 25 | 1,250 |
| Service / occupation | 15% | 20 | 12 | 240 |
| Carpathian borderland | 10% | 20 | 6 | 120 |
| Haze-era folk naming | 10% | 20 | 12 | 240 |
| **Total** | **100%** | | | **3,100** |

**Unique spellings across all methods:** 2,908, with 180 spellings generated more than once. This count assumes simple component concatenation as implemented by the supplied `join.style: "compound"`. It does not account for additional engine normalisation or user-authored joining exceptions.

## Editable files

- `01-method.json` — method-selection weights; 1d20
- `02-first-element.json` — geographic first elements; 1d100
- `03-second-element.json` — geographic endings; 1d100
- `04-founder.json` — people, historical surnames and founders; 1d100
- `05-founder-ending.json` — ownership/settlement suffixes; 1d100
- `06-occupation-root.json` — service/work first elements; 1d20
- `07-occupation-ending.json` — service/work endings; 1d12
- `08-borderland-root.json` — borderland first elements; 1d20
- `09-borderland-ending.json` — borderland endings; 1d12
- `10-haze-root.json` — Haze-era first elements; 1d20
- `11-haze-ending.json` — Haze-era endings; 1d12
- `naming-schemes.json` — method component references and regional routing
- `PREVIEW.csv` — example outputs for easy sorting/editing in VS Code or Excel

## Design approach

### Slavic / Central European (main 65%)
Most ordinary settlements use old geographical names or names inherited from families and landholders. Inspiration is taken from the **processes** of Slovak, Czech and Polish naming, not from indiscriminately transcribing those languages. Important historic forms include -ice / -ovice, -ov / -in, -ovce and occupational/community naming.

### Old occupations (15%)
Names that originated around guard posts, mills, mine camps, trades or service villages. The work may no longer survive.

### Carpathian borderlands (10%)
Fictional roots and endings inspired loosely by Romanian/Moldovan settlement-name structures. Deliberately a separate method from the main naming grammar. They are romanised for consistent plain-text editing, and **not linguistic reconstructions**.

### Haze-era (10%)
Retains geographical, pronounceable, fairly understated names instead of overwrought names such as 'Bloodgrave'. Some may be reinterpretations or renamings from an earlier time. Haze-related names are not restricted by Age or Environment yet.

## Why there are still awkward combinations

The currently uploaded JSON structure offers `join.style: "compound"` and `exceptions: []`, but does not document a syntax for suffix-family eligibility or phonological transformations. Rather than invent fields and break the loader, **this pack leaves those out**.

A realistic later expansion should add editable, physically-rollable **compatibility families** and explicit join transformations. That can prevent vowel collisions like `Luka + ova` without silently rerolling results. The data in this pack is intentionally editorial starter material rather than claims of perfectly accurate Slavic grammar.

## How to install

1. Make a backup of your current Bratislor naming directory and naming-schemes data.
2. Replace your corresponding five existing table JSON files plus the naming scheme file with the updated equivalents in this pack.
3. Add the six additional table files **wherever your existing naming table loader searches for JSON**.
4. If `naming-schemes.json` is a **shared, multi-region registry**, do not replace the whole file. Instead, copy only this pack's Bratislor entry into your existing `naming_schemes` array, preserving all other regional entries.
5. Reload/restart the application and run its table validation. If new tables are not discovered, check its content manifest/registry.
6. Validate in both CLI and GUI. Existing saved settlement snapshots should remain unchanged.

### Note on default content vs overrides

User overrides may mask edits to bundled JSON. If changes are not reflected, check the current app's override precedence before resetting or deleting any user-defined content.

## Further refinement before calling this finished

- Use separate root/suffix compatibility families, particularly for vowels and possessive suffixes.
- Keep a curated allow/block list with **documented** dice-compatible fallback rather than invisible rerolls.
- If the region develops confirmed ethnic/language borders, tie the borderland method to subregional tags. For now its 10% share is just a draft.
- Haze-era renaming should eventually depend on an explicit contamination/history tag, not assume all Bratislor settlements were renamed.
- Test name quality with full enumeration and a controlled editorial review.
