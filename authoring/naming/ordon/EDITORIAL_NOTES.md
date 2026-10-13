# Editorial policy and remaining checks

## Why the change

*Oswaldsworth* is the clearest example of treating linguistically possible parts as universally compatible. The core rule is to approve final settlement spellings before they can appear. For family names, industries and route names, the data now contains the final forms directly.

## Category-specific rules

- **Old English & Landscape (42%)**: two component rolls remain, but the suffix vocabulary is narrowed to common combinations. Check the remaining one thousand names in COMPLETE_CATALOG.csv; a few may still be stylistically subjective.
- **Founders / Manors (20%)**: only two curated historical-sounding forms per historic personal name. These are not precise historical derivations. Some are deliberately shortened (Oswald -> Oswalden), reflecting how historical place names can shorten over time.
- **Industry (18%)**: choose approved place names, never arbitrary combinations of facility + natural feature.
- **Transport (12%)**: use approved locality names, not random infrastructure nouns. Some towns in a Victorian-inspired industrial centre may have originated as junctions, wharves, etc.
- **Parish / Inherited (8%)**: full names are editorially reviewed and remain user-editable.

## Editorial checks

1. Read names aloud. Reject tongue-twisters or accidental comedy.
2. Ask whether it plausibly denotes an inhabited settlement, not just an individual factory/building/road.
3. Avoid overlong uncontracted personal names and awkward automatic possessive-s.
4. Prefer inherited local spellings to generically ominous fantasy labels.
5. Allow unusual but believable real-world homonyms: the presence of an existing British place of that spelling is not intrinsically an error.
6. Maintain physical dice trace and zero silent rejection sampling.

## Extension suggestion

Once these tables have been approved, more variety should come from **adding curated variants**, not reintroducing free-for-all suffix joins. The `approved_names` objects can be expanded with new physically weighted variant tables.
