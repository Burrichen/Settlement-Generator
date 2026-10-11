# Built-in content

Each `*.json` file in this folder, including subfolders, is a partial content
set:

- tables;
- tags;
- naming schemes;
- the generation `sequence`, in one file only.

The files are merged in order of their path and embedded into both binaries at
build time. They are read-only at runtime.

Settlement naming data is in `kurovia/naming/`; see
[docs/NAMING_SYSTEM.md](../docs/NAMING_SYSTEM.md).

- The content and its open questions are described in
  [docs/CONTENT.md](../docs/CONTENT.md).
- The format is described in [docs/DATA_MODEL.md](../docs/DATA_MODEL.md).
- User customisations are stored separately in the user database.
