import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

const projectRoot = resolve(import.meta.dirname, "..");

if (process.platform !== "darwin") {
  const result = spawnSync("npm", ["run", "desktop:dev"], {
    cwd: projectRoot,
    stdio: "inherit",
  });
  process.exit(result.status ?? 1);
}

// SFSpeechRecognition на macOS должен запускаться из настоящего пакета .app:
// только там TCC надёжно читает Contents/Info.plist с описанием разрешения.
const build = spawnSync("npm", ["run", "tauri", "build", "--", "--bundles", "app"], {
  cwd: projectRoot,
  stdio: "inherit",
});

if (build.status !== 0) process.exit(build.status ?? 1);

// Cargo workspace keeps one shared target directory at the project root.
// Do not use `-n`: it would force macOS to start a second C-3PO instance.
const appPath = resolve(projectRoot, "target/release/bundle/macos/C-3PO.app");
if (!existsSync(appPath)) {
  console.error(`Свежая сборка C-3PO не найдена: ${appPath}`);
  process.exit(1);
}

const application = spawnSync("open", [appPath], {
  cwd: projectRoot,
  stdio: "inherit",
});

if (application.status !== 0) process.exit(application.status ?? 1);

console.log(`C-3PO открыт: ${appPath}`);
