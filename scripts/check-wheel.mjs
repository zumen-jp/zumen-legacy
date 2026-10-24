import { execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const wheels = readdirSync(join(root, "target/wheels")).filter((name) => name.endsWith(".whl"));
if (wheels.length !== 1) throw new Error(`target/wheels の wheel は 1 つにしてください（現在 ${wheels.length} 個）`);
const directory = mkdtempSync(join(tmpdir(), "zumen wheel 日本語 "));
const python = join(directory, process.platform === "win32" ? "Scripts/python.exe" : "bin/python");
try {
  execFileSync(
    "uv",
    [
      "venv",
      directory,
      "--python",
      join(root, "bindings/python/.venv", process.platform === "win32" ? "Scripts/python.exe" : "bin/python"),
    ],
    { stdio: "inherit" },
  );
  execFileSync("uv", ["pip", "install", "--python", python, join(root, "target/wheels", wheels[0])], {
    stdio: "inherit",
  });
  execFileSync(python, [join(root, "bindings/python/tests/wheel_smoke.py"), join(root, "tests/fixtures"), directory], {
    cwd: directory,
    stdio: "inherit",
  });
} finally {
  rmSync(directory, { recursive: true, force: true });
}
