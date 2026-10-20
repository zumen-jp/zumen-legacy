import { execFileSync } from "node:child_process";
import { version } from "./wasm-bindgen-version.mjs";

execFileSync("cargo", ["install", "wasm-bindgen-cli", "--locked", "--version", version], { stdio: "inherit" });
