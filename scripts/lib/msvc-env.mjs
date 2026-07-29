import { access } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import path from "node:path";

const execFileAsync = promisify(execFile);
const VC_COMPONENT = "Microsoft.VisualStudio.Component.VC.Tools.x86.x64";

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
  const vswhere = path.join(
    programFilesX86,
    "Microsoft Visual Studio",
    "Installer",
    "vswhere.exe",
  );
  try {
    await access(vswhere);
    const { stdout } = await execFileAsync(vswhere, [
      "-latest",
      "-products",
      "*",
      "-requires",
      VC_COMPONENT,
      "-property",
      "installationPath",
    ]);
    const installationPath = stdout.trim();
    if (installationPath) {
      return {
        discovery: "vswhere-with-vc-component",
        vsDevCmd: path.join(
          installationPath,
          "Common7",
          "Tools",
          "VsDevCmd.bat",
        ),
      };
    }
  } catch {
    // A direct fallback is allowed only if its initialized tools are verified below.
  }

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
  if (!direct) {
    throw new Error(
      `Visual Studio installation with ${VC_COMPONENT} was not found.`,
    );
  }
  return { discovery: "direct-verified-fallback", vsDevCmd: direct };
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

async function verifyTool(environment, tool) {
  let output;
  try {
    const { stdout, stderr } = await execFileAsync(tool, ["/?"], {
      encoding: "utf8",
      env: environment,
      maxBuffer: 4 * 1024 * 1024,
      windowsHide: true,
    });
    output = `${stdout}${stderr}`;
  } catch (error) {
    output = `${error.stdout ?? ""}${error.stderr ?? ""}`;
  }
  const expected = tool === "cl.exe" ? /Optimizing Compiler/iu : /Incremental Linker/iu;
  if (!expected.test(output)) {
    throw new Error(`${tool} did not produce its expected help response.`);
  }
  const { stdout: resolved } = await execFileAsync("where.exe", [tool], {
    encoding: "utf8",
    env: environment,
    windowsHide: true,
  });
  const toolPath = resolved.split(/\r?\n/u).find(Boolean);
  if (!toolPath) throw new Error(`${tool} was not found in the MSVC environment.`);
  return toolPath;
}

export async function getMsvcEnvironmentDetails() {
  if (process.platform !== "win32") {
    return {
      clPath: null,
      discovery: "not-windows",
      environment: { ...process.env },
      linkPath: null,
      vsDevCmd: null,
    };
  }

  const { discovery, vsDevCmd } = await findVsDevCmd();
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
  const clPath = await verifyTool(environment, "cl.exe");
  const linkPath = await verifyTool(environment, "link.exe");
  return { clPath, discovery, environment, linkPath, vsDevCmd };
}

export async function getMsvcEnvironment() {
  return (await getMsvcEnvironmentDetails()).environment;
}
