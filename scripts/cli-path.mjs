import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** Respect Cargo configuration and CARGO_TARGET_DIR, including paths with spaces and non-ASCII characters. */
export function cliPath(profile = "release") {
  const metadata = JSON.parse(
    execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
      cwd: fileURLToPath(new URL("../", import.meta.url)),
      encoding: "utf8",
    }),
  );
  return join(metadata.target_directory, profile, process.platform === "win32" ? "zumen.exe" : "zumen");
}
