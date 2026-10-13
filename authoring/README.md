# Authoring material

Development and editorial resources for content packs. Nothing in this folder
is bundled into the application: only `content/` is. Keep previews, catalogues,
review notes and archived lists here, so they can never be loaded as tables.

## `naming/ordon/`

The Ordon Quality Pass v2 pack's tools. Its active tables are installed in
`content/kurovia/naming/ordon/`; see [docs/ORDON_NAMING.md](../docs/ORDON_NAMING.md).

| File                                                           | Purpose                                                                                                                                |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `AUTHORING_PREVIEW.py`                                         | Validates the installed tables and exports the catalogues: `python3 authoring/naming/ordon/AUTHORING_PREVIEW.py --samples 20 --export` |
| `COMPLETE_CATALOG.csv`                                         | Every generation path (rewritten by the script). The app's tests check it matches the engine                                           |
| `COLLISIONS.csv`, `PREVIEW.csv`                                | Spellings reachable by several paths; seeded sample rolls (rewritten by the script)                                                    |
| `EDITORIAL_FLAGS.csv`, `EDITORIAL_NOTES.md`                    | Names flagged for human review, and the editorial policy. Flags are not rejection rules                                                |
| `METHOD_WEIGHTS.csv`, `README.md`, `VALIDATION.md`             | The pack's own notes                                                                                                                   |
| `CLAUDE_INSTALL_PROMPT.md`                                     | The pack's install instructions                                                                                                        |
| `LEGACY_FOUNDER_ENDINGS.json`, `LEGACY_SUFFIX_VOCABULARY.json` | Archived reference only. **Never** active tables                                                                                       |
| `supplied/`                                                    | The pack's exports and scheme exactly as supplied, before installation, for comparison                                                 |
