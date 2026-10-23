// Writes the license texts of the Rust crates that are compiled into what zumen ships: the extension module of the
// Python package (bindings/python) and the two WebAssembly modules of @zumenjs/reader (bindings/wasm, and
// bindings/wasm-pdf for PDF, decision 0030). The licenses of MIT,
// BSD, Apache-2.0 and the like ask every copy of a binary to carry the copyright notices and the license texts of
// the code in it, and zumen's own LICENSE covers only zumen.
//
//   node scripts/notices.mjs           writes the files
//   node scripts/notices.mjs --check   fails when a file differs from what Cargo.lock gives (pnpm run check)
//
// The crates are those that are linked: the normal dependencies of the binding for the platforms it is built for,
// without build scripts, procedural macros and development dependencies, which run at build time and are not in the
// binary. Each crate's license files are copied as they are, and a text that several crates share is written once.
// Crates that embed data under licenses of its own keep those licenses in assets/ (the fonts and CMaps of hayro), and
// they are copied too.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const outputs = [
  {
    file: "bindings/python/licenses/THIRD_PARTY_LICENSES.txt",
    what: "the extension module of the Python package zumen",
    bindings: [{ manifest: "bindings/python/Cargo.toml", package: "zumen-python" }],
    // The platforms of the wheels (.github/workflows/python-wheels.yml)
    targets: [
      "x86_64-unknown-linux-gnu",
      "aarch64-unknown-linux-gnu",
      "aarch64-apple-darwin",
      "x86_64-pc-windows-msvc",
    ],
  },
  {
    file: "packages/reader/THIRD_PARTY_LICENSES.txt",
    what: "the WebAssembly modules of @zumenjs/reader (zumen_wasm_bg.wasm and zumen_wasm_pdf_bg.wasm)",
    bindings: [
      { manifest: "bindings/wasm/Cargo.toml", package: "zumen-wasm" },
      { manifest: "bindings/wasm-pdf/Cargo.toml", package: "zumen-wasm-pdf" },
    ],
    targets: ["wasm32-unknown-unknown"],
  },
];

const LICENSE_FILE = /^(licen[sc]e|copying|notice|unlicense|copyright)/i;
/** The license of data that a crate embeds, in its assets/ folder (LICENSE_FOXIT, CGATS_LICENSE.txt …) */
const ASSET_LICENSE_FILE = /licen[sc]e/i;

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
  if (!start) throw new Error(`${manifest} に ${packageName} がありません`);
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

function render({ what, bindings, targets }) {
  const crates = new Map();
  for (const { manifest, package: packageName } of bindings) {
    for (const target of targets) {
      for (const p of linkedCrates(manifest, packageName, target)) crates.set(`${p.name} ${p.version}`, p);
    }
  }
  const sorted = [...crates.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  /** text -> the crates that carry it */
  const texts = new Map();
  const licenseFiles = (/** @type {{ manifest_path: string }} */ p) =>
    readdirSync(path.dirname(p.manifest_path), { withFileTypes: true })
      .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
      .map((e) => e.name)
      .sort();
  for (const [key, p] of sorted) {
    let dir = path.dirname(p.manifest_path);
    let files = licenseFiles(p);
    let from = "";
    if (files.length === 0) {
      // A crate published without its license file (alloc-stdlib) is covered by the license of its repository, which
      // another crate of the same repository ships
      const sibling = sorted.find(([, other]) => other.repository === p.repository && licenseFiles(other).length > 0);
      if (!p.repository || !sibling) throw new Error(`${dir} に ${key} のライセンスファイルがありません`);
      dir = path.dirname(sibling[1].manifest_path);
      files = licenseFiles(sibling[1]);
      from = ` (the license of its repository, as ${sibling[0]} ships it)`;
    }
    const assets = path.join(dir, "assets");
    if (existsSync(assets)) {
      files.push(
        ...readdirSync(assets, { withFileTypes: true })
          .filter((e) => e.isFile() && ASSET_LICENSE_FILE.test(e.name))
          // The name is written into the file, so it uses / on every platform
          .map((e) => `assets/${e.name}`)
          .sort(),
      );
    }
    for (const name of files) {
      const text = readText(path.join(dir, name));
      if (!texts.has(text)) texts.set(text, []);
      texts.get(text).push(name.startsWith("assets") ? `${key} (${name}, the data it embeds)` : `${key}${from}`);
    }
  }
  const rule = "=".repeat(78);
  const lines = [
    `Licenses of the Rust crates that are compiled into ${what}.`,
    "",
    "zumen itself is under the MIT license (LICENSE). The crates below are linked into the binary, from",
    `${bindings.map((binding) => binding.manifest).join(" and ")} for ${targets.join(", ")}; build scripts, procedural`,
    "macros and development dependencies are not in it.",
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
  const current = existsSync(file) ? readFileSync(file, "utf8").replace(/\r\n?/g, "\n") : null;
  if (check) {
    if (current !== generated) {
      failed = true;
      console.error(`${output.file} が Cargo.lock と一致しません。pnpm run notices で更新してコミットしてください。`);
    } else {
      console.log(`ok   ${output.file}`);
    }
  } else if (current !== generated) {
    writeFileSync(file, generated);
    console.log(`書き込み: ${output.file}`);
  } else {
    console.log(`same ${output.file}`);
  }
}
if (failed) process.exit(1);
