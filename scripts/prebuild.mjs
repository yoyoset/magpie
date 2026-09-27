// Runs before `tauri build`: builds the UI and the `magpie` CLI, and stages the
// CLI where Tauri's externalBin expects it (binaries/magpie-<target-triple>.exe).
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const run = (cmd, cwd = root) => execSync(cmd, { cwd, stdio: "inherit" });

run("npm ci", join(root, "ui"));
run("npm run build", join(root, "ui"));
run("cargo build --release -p magpie-cli");

const triple = execSync("rustc -vV").toString().match(/^host: (.+)$/m)[1].trim();
const ext = process.platform === "win32" ? ".exe" : "";
mkdirSync(join(root, "app", "binaries"), { recursive: true });
copyFileSync(join(root, "target", "release", `magpie${ext}`), join(root, "app", "binaries", `magpie-${triple}${ext}`));
console.log(`staged magpie CLI for ${triple}`);
