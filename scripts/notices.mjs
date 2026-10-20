// Writes the license texts of the Rust crates that are compiled into what zumen ships: the extension module of the
// Python package (bindings/python) and the WebAssembly reader of @zumenjs/dwg (bindings/wasm). The licenses of MIT,
// BSD, Apache-2.0 and the like ask every copy of a binary to carry the copyright notices and the license texts of
// the code in it, and zumen's own LICENSE covers only zumen.
//
//   node scripts/notices.mjs           writes the files
//   node scripts/notices.mjs --check   fails when a file differs from what Cargo.lock gives (pnpm run check)
//
// The crates are those that are linked: the normal dependencies of the binding for the platforms it is built for,
// without build scripts, procedural macros and development dependencies, which run at build time and are not in the
// binary. Each crate's license files are copied as they are, and a text that several crates share is written once.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const outputs = [
  {
    file: "bindings/python/licenses/THIRD_PARTY_LICENSES.txt",
    what: "the extension module of the Python package zumen",
    manifest: "bindings/python/Cargo.toml",
    package: "zumen-python",
    // The platforms of the wheels (.github/workflows/python-wheels.yml)
    targets: ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin"],
  },
  {
    file: "packages/dwg/THIRD_PARTY_LICENSES.txt",
    what: "the WebAssembly reader of @zumenjs/dwg",
    manifest: "bindings/wasm/Cargo.toml",
    package: "zumen-wasm",
    targets: ["wasm32-unknown-unknown"],
  },
];

const LICENSE_FILE = /^(licen[sc]e|copying|notice|unlicense|copyright)/i;

function cargo(args) {
  return execFileSync("cargo", args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 1 << 30,
    stdio: ["ignore", "pipe", "inherit"],
  });
}

/** The crates from registries that `package` links for `target`, as cargo metadata describes them. */
function linkedCrates(manifest, packageName, target) {
  // cargo metadata gives the manifest path of a crate whether or not its sources are downloaded, and the license files
  // are in the sources
  cargo(["fetch", "--locked", "--manifest-path", manifest, "--target", target]);
  const metadata = JSON.parse(
    cargo(["metadata", "--format-version", "1", "--locked", "--manifest-path", manifest, "--filter-platform", target]),
  );
  const packages = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const start = metadata.packages.find((p) => p.name === packageName);
  if (!start) throw new Error(`${packageName} is not in ${manifest}`);
  const seen = new Set();
  const stack = [start.id];
  while (stack.length > 0) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id).deps) {
      // kind null is a normal dependency; "build" and "dev" are not in the binary
      if (!dep.dep_kinds.some((k) => k.kind === null)) continue;
      // a procedural macro runs when its user is built, and so do its own dependencies
      if (packages.get(dep.pkg).targets.some((t) => t.kind.includes("proc-macro"))) continue;
      stack.push(dep.pkg);
    }
  }
  // A crate without a registry source is one of ours (the workspace), covered by LICENSE
  return [...seen].map((id) => packages.get(id)).filter((p) => p.source !== null);
}

function readText(file) {
  const text = new TextDecoder("utf-8", { fatal: true }).decode(readFileSync(file));
  return text
    .replace(/\r\n?/g, "\n")
    .replace(/[ \t]+$/gm, "")
    .replace(/\n+$/, "");
}

function render({ what, manifest, package: packageName, targets }) {
  const crates = new Map();
  for (const target of targets) {
    for (const p of linkedCrates(manifest, packageName, target)) crates.set(`${p.name} ${p.version}`, p);
  }
  const sorted = [...crates.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  /** text -> the crates that carry it */
  const texts = new Map();
  for (const [key, p] of sorted) {
    const dir = path.dirname(p.manifest_path);
    const files = readdirSync(dir, { withFileTypes: true })
      .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
      .map((e) => e.name)
      .sort();
    if (files.length === 0) throw new Error(`${key} has no license file in ${dir}`);
    for (const name of files) {
      const text = readText(path.join(dir, name));
      if (!texts.has(text)) texts.set(text, []);
      texts.get(text).push(key);
    }
  }
  const rule = "=".repeat(78);
  const lines = [
    `Licenses of the Rust crates that are compiled into ${what}.`,
    "",
    "zumen itself is under the MIT license (LICENSE). The crates below are linked into the binary, from",
    `${manifest} for ${targets.join(", ")}; build scripts, procedural macros and development dependencies are not in it.`,
    "Generated by scripts/notices.mjs from Cargo.lock (pnpm run notices). Do not edit.",
    "",
    `Crates (${sorted.length}), with the license that each declares:`,
    "",
    ...sorted.map(([key, p]) => `  ${key} (${p.license ?? "no license field"})`),
    "",
    `License texts (${texts.size}), as the crates ship them; a text that several crates share is written once:`,
  ];
  let n = 0;
  for (const [text, users] of texts) {
    n += 1;
    lines.push("", rule, `Text ${n} of ${texts.size}, in the files of: ${users.join(", ")}`, rule, "", text);
  }
  return `${lines.join("\n")}\n`;
}

const check = process.argv.includes("--check");
let failed = false;
for (const output of outputs) {
  const generated = render(output);
  const file = path.join(root, output.file);
  const current = existsSync(file) ? readFileSync(file, "utf8") : null;
  if (check) {
    if (current !== generated) {
      failed = true;
      console.error(`${output.file} is out of date with Cargo.lock. Run pnpm run notices and commit it.`);
    } else {
      console.log(`ok   ${output.file}`);
    }
  } else if (current !== generated) {
    writeFileSync(file, generated);
    console.log(`wrote ${output.file}`);
  } else {
    console.log(`same ${output.file}`);
  }
}
if (failed) process.exit(1);
