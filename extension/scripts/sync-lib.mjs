// Copy the canonical shared lib (chrome/lib/udm-core.js) into every browser
// build so they never drift. Run after editing the core.
// Usage: node extension/scripts/sync-lib.mjs [--check]
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const source = resolve(root, "chrome/lib/udm-core.js");
const targets = [resolve(root, "firefox/lib/udm-core.js")];

const checkOnly = process.argv.includes("--check");
const src = readFileSync(source, "utf8");
let drift = false;

for (const t of targets) {
  let current = null;
  try { current = readFileSync(t, "utf8"); } catch {}
  if (current === src) {
    console.log(`in sync: ${t}`);
    continue;
  }
  if (checkOnly) {
    console.error(`OUT OF SYNC: ${t}`);
    drift = true;
  } else {
    mkdirSync(dirname(t), { recursive: true });
    writeFileSync(t, src);
    console.log(`updated: ${t}`);
  }
}

if (checkOnly && drift) process.exit(1);
console.log("done");
