# Claude Code — Install the Ordon quality-reviewed naming pack

Continue in the existing Settlement Generator repository. Read the current naming engine, its schema, user override strategy, and v0.2.0 naming docs before editing.

Install this pack's 10 Ordon JSON tables while preserving their stable IDs and result IDs wherever compatible. Merge only the `naming.kurovia.ordon` entry in `naming-schemes.json`, retaining all other regions.

## Required generic engine behaviour

Add `join.style = "curated_variant"` as an authorable join operation, not an Ordon-specific branch. With two rolled component results (`source`, `variant`), read `source.approved_names[variant.id]` and output that string exactly. The `source_field` property specifies which source field holds the map; `variant_from_result_id = 1` indicates the second result's ID. Fail content validation on missing or empty mapping, never silently reroll or concatenate. Record both physical dice rolls and source IDs in generation traces.

Keep `compound` join and explicit spelling exceptions for traditional geographic names. Preserve the manual roll behaviour, GUI/TUI parity, God Mode, stable saved snapshots, user modifications, and update-safe migration. Do not change method probabilities.

Check that editing an `approved_names` variant in View Tables or raw JSON is supported or that the editor gives a clear safe fallback. The 1d4 source variants should show readable approved outputs when available.

Acceptance checks:
1. All tables parse and dice outcomes are fully covered.
2. Oswald + A = Oswalden; Oswald + B = Oswaldwick, not Oswaldsworth.
3. Godwin + A = Godwinsbury; Nerys + A = Nerysbury.
4. Foundry + A = Foulden (never Foundrywell).
5. Market + A = Market Cross (never Market Market).
6. Founder, industrial and route have no implicit random recombination.
7. Existing saved settlements, other regional naming schemes and all user overrides remain unchanged.
8. Run tests and generate/export a full name catalogue.

Do not add a new sixth naming method or invent canon. This is a realism/quality pass only.
