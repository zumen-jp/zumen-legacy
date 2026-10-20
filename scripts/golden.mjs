import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Invoke Node directly: neither POSIX environment assignments nor Windows .cmd shims are needed.
const result = spawnSync(
  process.execPath,
  [
    fileURLToPath(new URL("./vitest.mjs", import.meta.resolve("vitest/package.json"))),
    "run",
    "--project",
    "node",
    ...process.argv.slice(2),
  ],
  { stdio: "inherit", env: { ...process.env, UPDATE_GOLDEN: "1" } },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
