// Rewrites latest.json in the GitHub Release upload folder, for example to
// change the release notes after a build. `npm run release:build` already
// writes it. Usage: npm run release:manifest [-- --notes notes.md]
// See docs/RELEASING.md.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { artifacts, updaterManifest, workspaceVersion, writeJson } from "./release-lib.mjs";

const version = workspaceVersion();
const files = artifacts(version);
if (!existsSync(`${files.installer}.sig`)) {
  console.error(`Missing ${files.installer}.sig. Run npm run release:build first.`);
  process.exit(1);
}
const notesIndex = process.argv.indexOf("--notes");
const notes = notesIndex === -1 ? "" : readFileSync(process.argv[notesIndex + 1], "utf8").trim();
const target = join(files.uploadDir, "latest.json");
writeJson(target, updaterManifest({ version, installer: files.installer, notes }));
console.log(`Wrote ${target} for v${version}.`);
