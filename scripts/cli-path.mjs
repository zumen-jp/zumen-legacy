import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** 空白・日本語を含むパスでも、Cargo の設定と CARGO_TARGET_DIR に従う。 */
export function cliPath(profile = "release") {
  const metadata = JSON.parse(
    execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
      cwd: fileURLToPath(new URL("../", import.meta.url)),
      encoding: "utf8",
    }),
  );
  return join(metadata.target_directory, profile, process.platform === "win32" ? "zumen.exe" : "zumen");
}
