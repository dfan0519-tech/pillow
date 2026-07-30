import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const environment = await getMsvcEnvironment();
const child = spawn(
  process.platform === "win32" ? "cmd.exe" : "pnpm",
  process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm", "--filter", "@pillow/desktop", "tauri", "build", "--no-bundle"]
    : ["--filter", "@pillow/desktop", "tauri", "build", "--no-bundle"],
  {
    cwd: repositoryRoot,
    env: environment,
    stdio: "inherit",
  },
);

child.on("exit", (code, signal) => {
  if (signal) process.exit(1);
  process.exit(code ?? 1);
});
