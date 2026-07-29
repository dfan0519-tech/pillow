import { access } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import path from "node:path";

const execFileAsync = promisify(execFile);

async function firstExisting(paths) {
  for (const candidate of paths) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try the next installed Visual Studio edition.
    }
  }
  return null;
}

async function findVsDevCmd() {
  const programFilesX86 =
    process.env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)";
  const visualStudioRoot = path.join(
    programFilesX86,
    "Microsoft Visual Studio",
    "2022",
  );
  const direct = await firstExisting(
    ["BuildTools", "Community", "Professional", "Enterprise"].map((edition) =>
      path.join(visualStudioRoot, edition, "Common7", "Tools", "VsDevCmd.bat"),
    ),
  );
  if (direct) return direct;

  const vswhere = path.join(
    programFilesX86,
    "Microsoft Visual Studio",
    "Installer",
    "vswhere.exe",
  );
  await access(vswhere);
  const { stdout } = await execFileAsync(vswhere, [
    "-latest",
    "-products",
    "*",
    "-requires",
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    "-property",
    "installationPath",
  ]);
  const installationPath = stdout.trim();
  if (!installationPath) {
    throw new Error("Visual Studio C++ Build Tools were not found.");
  }
  return path.join(installationPath, "Common7", "Tools", "VsDevCmd.bat");
}

function parseEnvironment(stdout) {
  const environment = {};
  for (const line of stdout.split(/\r?\n/u)) {
    const separator = line.indexOf("=");
    if (separator <= 0) continue;
    environment[line.slice(0, separator)] = line.slice(separator + 1);
  }
  return environment;
}

export async function getMsvcEnvironment() {
  if (process.platform !== "win32") return { ...process.env };

  const vsDevCmd = await findVsDevCmd();
  const { stdout } = await execFileAsync(
    "cmd.exe",
    [
      "/d",
      "/s",
      "/c",
      `"call "${vsDevCmd}" -no_logo -arch=x64 -host_arch=x64 && set"`,
    ],
    {
      encoding: "utf8",
      maxBuffer: 4 * 1024 * 1024,
      windowsVerbatimArguments: true,
    },
  );
  const environment = { ...process.env, ...parseEnvironment(stdout) };
  const cargoBin = path.join(process.env.USERPROFILE ?? "", ".cargo", "bin");
  environment.Path = `${cargoBin};${environment.Path ?? environment.PATH ?? ""}`;
  environment.PATH = environment.Path;
  return environment;
}
