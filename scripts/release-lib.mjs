// Shared helpers for the release scripts. See docs/RELEASING.md.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Toolchain release builds must use, so artifacts can be rebuilt exactly. */
export const RELEASE_TOOLCHAIN = { rustc: "1.98.0", nodeMajor: 22 };

/** The CLI binary installed beside the GUI. */
export const CLI_NAME = "settlement-generator-cli";

/** The canonical version, from the workspace Cargo.toml. */
export function workspaceVersion() {
  const manifest = readFileSync(join(root, "Cargo.toml"), "utf8");
  const section = manifest.split("[workspace.package]")[1] ?? "";
  const match = section.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!match) throw new Error("No version under [workspace.package] in Cargo.toml");
  return match[1];
}

export function tauriConfig() {
  return JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
}

/** `owner/repo` from the updater endpoint. */
export function githubRepo(config = tauriConfig()) {
  const endpoint = config.plugins?.updater?.endpoints?.[0] ?? "";
  const match = endpoint.match(/^https:\/\/github\.com\/([^/]+\/[^/]+)\/releases\//);
  if (!match) throw new Error(`Updater endpoint is not a GitHub Releases URL: ${endpoint}`);
  return match[1];
}

/** GitHub replaces spaces in uploaded asset names with dots. */
export function githubAssetName(fileName) {
  return fileName.replaceAll(" ", ".");
}

/** Release outputs, as Tauri and these scripts name them. */
export function artifacts(version, config = tauriConfig()) {
  const release = join(root, "target", "release");
  return {
    installer: join(release, "bundle", "nsis", `${config.productName}_${version}_x64-setup.exe`),
    cli: join(release, `${CLI_NAME}.exe`),
    gui: join(release, `${config.mainBinaryName}.exe`),
    bundleDir: join(release, "bundle"),
    /** Exactly what is uploaded to the GitHub Release. */
    uploadDir: join(release, "github-release", `v${version}`),
  };
}

// npm and npx are .cmd scripts on Windows, which need a shell.
const needsShell = (command) => process.platform === "win32" && /^(npm|npx)$/.test(command);

export function run(command, args, options = {}) {
  console.log(`> ${command} ${args.join(" ")}`);
  execFileSync(command, args, {
    cwd: root,
    stdio: "inherit",
    shell: needsShell(command),
    ...options,
  });
}

export function output(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    shell: needsShell(command),
  }).trim();
}

/**
 * Everything that stops a publishable release, as readable messages.
 * `allowPlaceholders` skips only the unconfigured key/owner checks, for
 * packaging test builds that must never be published.
 */
export function readinessProblems({ tag, allowPlaceholders = false } = {}) {
  const problems = [];
  const version = workspaceVersion();
  const config = tauriConfig();
  const updater = config.plugins?.updater ?? {};

  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    problems.push(
      `Version ${version} is not a stable MAJOR.MINOR.PATCH version; ` +
        "the updater only offers stable releases.",
    );
  }
  if ("version" in config) {
    problems.push("tauri.conf.json must not set `version`; it comes from Cargo.toml.");
  }
  if (!allowPlaceholders) {
    const decoded = Buffer.from(updater.pubkey ?? "", "base64").toString("utf8");
    if (!decoded.includes("minisign public key")) {
      problems.push(
        "plugins.updater.pubkey is not a Tauri updater public key. " +
          "Paste the contents of your .key.pub file (see docs/RELEASING.md).",
      );
    }
    try {
      if (githubRepo(config).includes("REPLACE_WITH")) {
        problems.push("plugins.updater.endpoints still names a placeholder GitHub owner.");
      }
    } catch (e) {
      problems.push(String(e.message ?? e));
    }
  }
  if (updater.requireSignedVersion !== true) {
    problems.push("plugins.updater.requireSignedVersion must be true.");
  }
  if (config.bundle?.createUpdaterArtifacts !== true) {
    problems.push("bundle.createUpdaterArtifacts must be true.");
  }
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
    problems.push(
      "TAURI_SIGNING_PRIVATE_KEY is not set. Set it (and TAURI_SIGNING_PRIVATE_KEY_PASSWORD) " +
        "in this shell only, from your private key kept outside the repository.",
    );
  }
  if (tag !== undefined && tag !== `v${version}`) {
    problems.push(`Tag ${tag} does not match the version: expected v${version}.`);
  }
  return problems;
}

/** Problems with the build toolchain, compared with RELEASE_TOOLCHAIN. */
export function toolchainProblems() {
  const problems = [];
  const rustc = output("rustc", ["--version"]);
  if (!rustc.startsWith(`rustc ${RELEASE_TOOLCHAIN.rustc} `)) {
    problems.push(
      `Release builds use rustc ${RELEASE_TOOLCHAIN.rustc}, found "${rustc}". ` +
        `Install it with: rustup toolchain install ${RELEASE_TOOLCHAIN.rustc}, ` +
        `then run with RUSTUP_TOOLCHAIN=${RELEASE_TOOLCHAIN.rustc}.`,
    );
  }
  const nodeMajor = Number(process.versions.node.split(".")[0]);
  if (nodeMajor !== RELEASE_TOOLCHAIN.nodeMajor) {
    problems.push(
      `Release builds use Node.js ${RELEASE_TOOLCHAIN.nodeMajor}.x, found ${process.version}.`,
    );
  }
  return problems;
}

/**
 * The updater manifest (latest.json) for the signed installer, pointing at
 * the GitHub Release assets for `version`.
 */
export function updaterManifest({ version, installer, notes = "", config = tauriConfig() }) {
  const signature = readFileSync(`${installer}.sig`, "utf8").trim();
  const asset = encodeURIComponent(githubAssetName(basename(installer)));
  const nsis = {
    signature,
    url: `https://github.com/${githubRepo(config)}/releases/download/v${version}/${asset}`,
  };
  return {
    version,
    notes,
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: { "windows-x86_64": nsis, "windows-x86_64-nsis": nsis },
  };
}

export function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}
