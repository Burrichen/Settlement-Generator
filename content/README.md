# Built-in content

Each `*.json` file in this folder, including subfolders, is a partial content
set:

- tables;
- tags;
- naming and role schemes;
- shared results (`shared-results.json` in the folder of the tables that list
  them);
- the generation pipeline (the `start` flow), in `00-pipeline.json` only.

The framework (flows, shared results, scales, settings, objects, modifiers)
is described in [docs/FRAMEWORK.md](../docs/FRAMEWORK.md).

**One table per file.** Every table has its own file, together with the tags
it emits. A feature with several tables is a numbered folder (e.g.
`10-authority/01-authority.json`, `02-authority-representation.json`, …) so
the merge order, and so which scheme applies first, stays the same. Schemes,
and tags no table emits, have their own files in that folder
(`naming-schemes.json`, `role-schemes.json`, `tags.json`).

The files are merged in order of their path and embedded into both binaries at
build time. They are read-only at runtime.

Settlement naming data is in `kurovia/naming/<region>/`, and Authority
Figure names in `people/naming/<species>/`; see
[docs/NAMING_SYSTEM.md](../docs/NAMING_SYSTEM.md).

- The content and its open questions are described in
  [docs/CONTENT.md](../docs/CONTENT.md).
- The format is described in [docs/DATA_MODEL.md](../docs/DATA_MODEL.md).
- User customisations are stored separately in the user database.
