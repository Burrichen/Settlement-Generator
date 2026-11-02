# Test fixtures

- `v0.1.1.sqlite3`: a database written by the released v0.1.1 code itself
  (tag `v0.1.1`), holding a Town ("Riverside"), a City with a Unicode name
  ("Ærøskøbing – Ōshū 城") and a Village ("Little Hollow"). It predates maps,
  Notable Locations and districts. `tests/compatibility.rs` opens a copy of
  it with this build. Never edit it: regenerate it from the tag instead.
