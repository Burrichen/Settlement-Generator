// Builds the signed Windows release from a clean state and collects exactly
// the files to upload to the GitHub Release.
//
// Usage:
//   npm run release:build [-- --notes notes.md]
//   npm run release:build -- --packaging-test --test-pubkey <key.pub>
//     (an unpublishable test build, signed with a throwaway key)
//
// Steps:
//   1. Refuse to start unless the release is configured, the signing key is
//      in the environment, and the pinned toolchain is in use.
//   2. Remove old outputs, then reinstall npm packages exactly from
//      package-lock.json. Cargo builds use --locked.
//   3. Build the CLI (settlement-generator-cli.exe) and stage it as the
//      installer's sidecar, then build the GUI and the signed NSIS installer.
//   4. Check the outputs and write target/release/github-release/vX.Y.Z/:
//      installer, signature, latest.json and SHA256SUMS.txt.
//
// See docs/RELEASING.md.
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import {
  CLI_NAME,
  artifacts,
  githubAssetName,
  output,
  readinessProblems,
  root,
  run,
  tauriConfig,
  toolchainProblems,
  updaterManifest,
  workspaceVersion,
  writeJson,
} from "./release-lib.mjs";

const args = process.argv.slice(2);
const packagingTest = args.includes("--packaging-test");
const valueOf = (flag) => {
  const i = args.indexOf(flag);
  return i === -1 ? undefined : args[i + 1];
};
const notesFile = valueOf("--notes");
const notes = notesFile ? readFileSync(notesFile, "utf8").trim() : "";
const testPubkeyFile = valueOf("--test-pubkey");
if (packagingTest !== Boolean(testPubkeyFile)) {
  console.error("--packaging-test and --test-pubkey <key.pub> must be used together.");
  process.exit(1);
}

const version = workspaceVersion();
const config = tauriConfig();
const files = artifacts(version, config);

// 1. Preconditions.
const problems = [
  ...readinessProblems({ allowPlaceholders: packagingTest }),
  ...toolchainProblems(),
];
if (problems.length > 0) {
  console.error(`Cannot build release v${version}:`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}

// 2. Clean state and exact dependencies.
const sidecarDir = join(root, "src-tauri", "binaries");
for (const dir of [files.bundleDir, join(files.uploadDir, ".."), sidecarDir]) {
  rmSync(dir, { recursive: true, force: true });
}
run("npm", ["ci"]);

// The same inputs give the same binaries: no incremental state, no linker
// timestamps (/Brepro), and no machine-specific paths embedded.
const cargoHome = process.env.CARGO_HOME ?? join(process.env.USERPROFILE ?? "", ".cargo");
const env = {
  ...process.env,
  CARGO_INCREMENTAL: "0",
  CARGO_ENCODED_RUSTFLAGS: [
    "-Clink-arg=/Brepro",
    `--remap-path-prefix=${root}=settlement-generator`,
    `--remap-path-prefix=${cargoHome}=cargo-home`,
  ].join("\x1f"),
};

// 3. Build: the CLI first, staged where Tauri expects the sidecar.
run("cargo", ["build", "--release", "--locked", "-p", "settlement-cli"], { env });
const host = output("rustc", ["-vV"]).match(/^host: (.+)$/m)?.[1];
if (!host) throw new Error("Could not read the Rust host target");
mkdirSync(sidecarDir, { recursive: true });
copyFileSync(files.cli, join(sidecarDir, `${CLI_NAME}-${host}.exe`));
// Tauri verifies the new signature against the configured public key, so a
// release cannot be signed with a key the app would not trust. A packaging
// test supplies its throwaway public key through a temporary overlay; the
// committed configuration is never changed.
let releaseConfig = "src-tauri/tauri.release.conf.json";
if (packagingTest) {
  const overlay = JSON.parse(readFileSync(join(root, releaseConfig), "utf8"));
  overlay.plugins = { updater: { pubkey: readFileSync(testPubkeyFile, "utf8").trim() } };
  releaseConfig = join(root, "target", "packaging-test.conf.json");
  writeJson(releaseConfig, overlay);
}
run("npx", ["tauri", "build", "--ci", "--config", releaseConfig, "--", "--locked"], { env });

// 4. Check and collect.
for (const required of [files.gui, files.cli, files.installer, `${files.installer}.sig`]) {
  if (!existsSync(required)) throw new Error(`Expected build output is missing: ${required}`);
}
const cliVersion = output(files.cli, ["--version"]);
if (cliVersion !== `${CLI_NAME} ${version}`) {
  throw new Error(`The CLI reports "${cliVersion}", expected "${CLI_NAME} ${version}"`);
}

const uploadDir = packagingTest
  ? `${files.uploadDir}-PACKAGING-TEST-DO-NOT-PUBLISH`
  : files.uploadDir;
mkdirSync(uploadDir, { recursive: true });
const installerAsset = githubAssetName(basename(files.installer));
copyFileSync(files.installer, join(uploadDir, installerAsset));
copyFileSync(`${files.installer}.sig`, join(uploadDir, `${installerAsset}.sig`));
writeJson(
  join(uploadDir, "latest.json"),
  updaterManifest({ version, installer: files.installer, notes, config }),
);
const sums = [installerAsset, `${installerAsset}.sig`]
  .map((name) => {
    const hash = createHash("sha256").update(readFileSync(join(uploadDir, name)));
    return `${hash.digest("hex")}  ${name}`;
  })
  .join("\n");
writeFileSync(join(uploadDir, "SHA256SUMS.txt"), `${sums}\n`);

// Recorded beside (not inside) the upload folder, for rebuilding later.
writeJson(join(uploadDir, "..", `build-info-v${version}.json`), {
  version,
  rustc: output("rustc", ["--version"]),
  cargo: output("cargo", ["--version"]),
  node: process.version,
  npm: output("npm", ["--version"]),
  tauri: output("npx", ["tauri", "--version"]),
  host,
  built_at: new Date().toISOString(),
  packaging_test: packagingTest,
});

console.log(`\nRelease v${version} is in ${uploadDir}`);
console.log(sums);
if (packagingTest) {
  console.log(
    "\nPACKAGING TEST BUILD: the updater is not configured for publishing. Do not upload it.",
  );
}
