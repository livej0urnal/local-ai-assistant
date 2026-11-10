import { cpSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";

const [targetTriple, buildDirectory = "vendor/llama.cpp/build"] = process.argv.slice(2);
if (!targetTriple) throw new Error("Usage: node scripts/stage-llama-sidecar.mjs <target-triple> [build-directory]");

const executable = process.platform === "win32" ? "llama-server.exe" : "llama-server";

function findFile(directory) {
  if (!existsSync(directory)) return null;
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) {
      const nested = findFile(path);
      if (nested) return nested;
    } else if (basename(path) === executable) {
      return path;
    }
  }
  return null;
}

const source = findFile(resolve(buildDirectory));
if (!source) throw new Error(`${executable} was not found in ${buildDirectory}`);

const outputDirectory = resolve("src-tauri/binaries");
mkdirSync(outputDirectory, { recursive: true });
const suffix = process.platform === "win32" ? ".exe" : "";
const destination = join(outputDirectory, `llama-server-${targetTriple}${suffix}`);
cpSync(source, destination);
console.log(`Staged ${source} -> ${destination}`);
