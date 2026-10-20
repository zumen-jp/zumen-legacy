import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";
import { cliPath } from "../cli-path.mjs";
import { wasmBindgenVersion } from "../wasm-bindgen-version.mjs";

const directory = mkdtempSync(join(tmpdir(), "zumen 日本語 "));
after(() => rmSync(directory, { recursive: true, force: true }));
const binary = join(directory, process.platform === "win32" ? "図面 cli.exe" : "図面 cli");
copyFileSync(cliPath("debug"), binary);
const drawing = join(directory, "図面 01.DXF");
copyFileSync(new URL("../../tests/fixtures/dxf/layout.dxf", import.meta.url), drawing);
const unknown = join(directory, "readme.txt");
writeFileSync(unknown, "not a drawing");
const broken = join(directory, "broken.dwg");
writeFileSync(broken, "AC1032");
copyFileSync(drawing, join(directory, "broken.dxf"));

function parity(files, executable = binary) {
  return spawnSync(process.execPath, [fileURLToPath(new URL("../parity.mjs", import.meta.url)), ...files], {
    env: { ...process.env, ZUMEN_BINARY: executable },
    encoding: "utf8",
  });
}

test("reads the locked wasm-bindgen version with LF or CRLF", () => {
  for (const newline of ["\n", "\r\n"]) {
    assert.equal(
      wasmBindgenVersion(["[[package]]", 'name = "wasm-bindgen"', 'version = "0.2.129"'].join(newline)),
      "0.2.129",
    );
  }
  assert.throws(() => wasmBindgenVersion('name = "wasm-bindgen-test"'));
});

test("parity runs a native executable and all views through Japanese paths with spaces", () => {
  const result = parity([drawing, unknown]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /same .*@Layout1/);
  assert.match(result.stdout, /skip .*unknown format/);
});

test("a missing executable is a failure, not an unsupported drawing", () => {
  const result = parity([drawing], join(directory, "missing.exe"));
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Rust CLI not found/);
});

test("a recognised but broken drawing fails even alongside a valid drawing", () => {
  const result = parity([drawing, broken]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /ERROR .*broken.dwg/);
});

test("an empty input directory or only unsupported files cannot pass", () => {
  const empty = mkdtempSync(join(directory, "empty "));
  for (const files of [[empty], [unknown]]) {
    const result = parity(files);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /No drawings were compared/);
  }
});
