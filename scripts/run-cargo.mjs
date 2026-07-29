import { spawn } from "node:child_process";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error("Usage: node scripts/run-cargo.mjs <cargo arguments>");
  process.exit(2);
}

const child = spawn("cargo", args, {
  cwd: new URL("..", import.meta.url),
  env: await getMsvcEnvironment(),
  stdio: "inherit",
  windowsHide: true,
});

child.on("error", (error) => {
  console.error(error.message);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    console.error(`cargo terminated by signal ${signal}`);
    process.exit(1);
  }
  process.exit(code ?? 1);
});
