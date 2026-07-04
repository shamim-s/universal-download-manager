// Copies the built daemon into src-tauri/binaries/ with the Tauri sidecar naming
// convention (`udm-daemon-<target-triple><ext>`) so `externalBin` picks it up.
//
// Run after building the daemon release:
//   cd daemon && cargo build --release
//   cd ui && node scripts/prepare-sidecar.mjs
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, copyFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(here, "..");
const repoRoot = resolve(uiRoot, "..");

// Host target triple from rustc (e.g. x86_64-pc-windows-msvc).
const triple = execSync("rustc -Vv")
  .toString()
  .split("\n")
  .find((l) => l.startsWith("host:"))
  ?.split(/\s+/)[1];

if (!triple) {
  console.error("Could not determine the Rust host triple. Is rustc on PATH?");
  process.exit(1);
}

const ext = process.platform === "win32" ? ".exe" : "";
const src = join(repoRoot, "daemon", "target", "release", `udm-daemon${ext}`);
const destDir = join(uiRoot, "src-tauri", "binaries");
const dest = join(destDir, `udm-daemon-${triple}${ext}`);

if (!existsSync(src)) {
  console.error(`Daemon binary not found at ${src}.\nBuild it first: cd daemon && cargo build --release`);
  process.exit(1);
}

mkdirSync(destDir, { recursive: true });
copyFileSync(src, dest);
console.log(`Sidecar ready: ${dest}`);
