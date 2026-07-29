import { access, readFile } from "node:fs/promises";

const required = [
  "Cargo.toml",
  "package.json",
  "pnpm-workspace.yaml",
  "apps/desktop/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
];

for (const file of required) await access(new URL(`../${file}`, import.meta.url));

const rootPackage = JSON.parse(
  await readFile(new URL("../package.json", import.meta.url), "utf8"),
);
if (rootPackage.private !== true) throw new Error("root workspace must be private");
if (rootPackage.packageManager !== "pnpm@10.14.0") {
  throw new Error("pnpm version is not locked");
}
console.log("workspace structure ok");
