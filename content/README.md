# Built-in content

Each `*.json` file in this folder is a partial content set (tables, tags and,
in one file, the generation `sequence`). The files are merged in file-name
order and embedded into both binaries at build time. They are read-only at
runtime.

- The content and its open questions are described in
  [docs/CONTENT.md](../docs/CONTENT.md).
- The format is described in [docs/DATA_MODEL.md](../docs/DATA_MODEL.md).
- User customisations are stored separately in the user database.
