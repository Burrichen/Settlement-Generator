// Sets the application version in its single source (the workspace
// Cargo.toml) and updates Cargo.lock to match.
// Usage: npm run version:bump -- X.Y.Z
// See docs/VERSIONING.md and docs/RELEASING.md.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { root, run, workspaceVersion } from "./release-lib.mjs";

const next = process.argv[2];
const current = workspaceVersion();
const parse = (v) => v.split(".").map(Number);

if (!next || !/^\d+\.\d+\.\d+$/.test(next)) {
  console.error("Usage: npm run version:bump -- X.Y.Z (a stable version, no 'v' prefix)");
  process.exit(1);
}
const [a, b] = [parse(current), parse(next)];
const newer = b[0] - a[0] || b[1] - a[1] || b[2] - a[2];
if (newer <= 0) {
  console.error(`${next} is not newer than the current version ${current}.`);
  process.exit(1);
}

const manifestPath = join(root, "Cargo.toml");
const manifest = readFileSync(manifestPath, "utf8");
const [before, after] = manifest.split("[workspace.package]");
const updated = after.replace(/^(\s*version\s*=\s*)"[^"]+"/m, `$1"${next}"`);
writeFileSync(manifestPath, `${before}[workspace.package]${updated}`);
if (workspaceVersion() !== next) throw new Error("Cargo.toml was not updated as expected");

// Only the workspace's own entries in Cargo.lock change.
run("cargo", ["update", "--workspace", "--offline"]);

console.log(`\nVersion ${current} -> ${next}.`);
console.log(
  "Next: run the full check suite, then follow the release checklist in docs/RELEASING.md.",
);
