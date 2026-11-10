import { spawn, spawnSync } from "node:child_process";
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

const appPath = resolve(projectRoot, "src-tauri/target/release/bundle/macos/C-3PO.app");
const application = spawn("open", ["-n", appPath], {
  cwd: projectRoot,
  detached: true,
  stdio: "ignore",
});
application.unref();

console.log(`C-3PO открыт: ${appPath}`);
