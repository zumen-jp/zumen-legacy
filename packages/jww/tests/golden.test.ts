import { readDrawing, sceneToJson } from "@zumenjs/core";
import { describe, it } from "vitest";
import { matchGolden } from "../../../tests/golden.js";
import { jww } from "../src/index.js";
import { build, buildImages, buildSolids } from "./fixture.js";

// Rust の読み取り（crates/zumen-jww）も同じ図面を読み、同じ Scene の JSON になることを確かめる
describe("JWW の正解ファイル", () => {
  it.each([200, 600, 700])("版 %i の図面と Scene", (version) => {
    const bytes = build(version);
    matchGolden(`jww/minimal-v${version}.jww`, bytes);
    matchGolden(`jww/minimal-v${version}.scene.json`, `${sceneToJson(readDrawing(bytes, [jww]), 2)}\n`);
  });
  it.each([600, 700])("版 %i の画像の配置", (version) => {
    const bytes = buildImages(version);
    matchGolden(`jww/images-v${version}.jww`, bytes);
    matchGolden(`jww/images-v${version}.scene.json`, `${sceneToJson(readDrawing(bytes, [jww]), 2)}\n`);
  });
  it.each([600, 700])("版 %i のソリッドの輪郭", (version) => {
    const bytes = buildSolids(version);
    matchGolden(`jww/solids-v${version}.jww`, bytes);
    matchGolden(`jww/solids-v${version}.scene.json`, `${sceneToJson(readDrawing(bytes, [jww]), 2)}\n`);
  });
});
