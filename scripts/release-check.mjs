// Checks that a signed release can be built and published.
// Usage: npm run release:check [-- --tag vX.Y.Z]
// See docs/RELEASING.md. Exits non-zero, listing every problem, if not ready.
import { readinessProblems, toolchainProblems, workspaceVersion } from "./release-lib.mjs";

const tagIndex = process.argv.indexOf("--tag");
const tag = tagIndex === -1 ? undefined : process.argv[tagIndex + 1];
const version = workspaceVersion();
const problems = [...readinessProblems({ tag }), ...toolchainProblems()];

if (problems.length > 0) {
  console.error(`Not ready to release v${version}:`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}
console.log(`Ready to build and publish v${version}.`);
