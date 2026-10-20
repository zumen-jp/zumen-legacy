/// <reference types="vite/client" />
import { afterEach, describe, expect, it } from "vitest";
import r2000 from "../../../tests/fixtures/dwg/r2000.dwg?url";
import r2018 from "../../../tests/fixtures/dwg/r2018.dwg?url";
import r12 from "../../../tests/fixtures/dxf/r12.dxf?url";
import jww from "../../../tests/fixtures/jww/minimal-v600.jww?url";
import { LimitError, ReaderWorker } from "../src/index.js";

// ページから Web Worker（src/worker.ts）を作って読む。Vite は new Worker(new URL(…, import.meta.url)) を worker にする
const createWorker = () => new Worker(new URL("../src/worker.ts", import.meta.url), { type: "module" });
// 図面は Vite の ?url で配信させる（そのままの URL では、ブラウザのテストのページが返る）
const urls: Record<string, string> = {
  "dwg/r2000.dwg": r2000,
  "dwg/r2018.dwg": r2018,
  "dxf/r12.dxf": r12,
  "jww/minimal-v600.jww": jww,
};
const fixture = async (name: string) => new Uint8Array(await (await fetch(urls[name] as string)).arrayBuffer());

let workers: ReaderWorker[] = [];
const open = () => {
  let created = 0;
  const reader = new ReaderWorker(() => {
    created++;
    return createWorker();
  });
  workers.push(reader);
  return { reader, created: () => created };
};

afterEach(() => {
  for (const reader of workers) reader.terminate();
  workers = [];
});

describe("ReaderWorker", () => {
  it("Worker の生成が例外になっても、次の読み取りを処理する", async () => {
    let created = 0;
    const reader = new ReaderWorker(() => {
      if (created++ === 0) throw new Error("Worker could not start");
      return createWorker();
    });
    workers.push(reader);
    const bytes = await fixture("dxf/r12.dxf");
    await expect(reader.read(bytes)).rejects.toThrow("Worker could not start");
    expect((await reader.read(bytes)).format).toBe("dxf");
  });

  it("転送できない入力を拒否し、待っている次の図面を読む", async () => {
    const { reader } = open();
    const bytes = await fixture("dxf/r12.dxf");
    const detached = bytes.slice();
    structuredClone(detached, { transfer: [detached.buffer] });
    const failed = reader.read(detached);
    const next = reader.read(bytes);
    await expect(failed).rejects.toThrow();
    expect((await next).format).toBe("dxf");
  });

  it("Web Worker で読み、点列が Float64Array の Scene を返す", async () => {
    const { reader } = open();
    const scene = await reader.read(await fixture("dwg/r2000.dwg"), { view: "Layout1" });
    expect(scene.format).toBe("dwg");
    const primitive = scene.entities.flatMap((entity) => entity.primitives).find((item) => item.kind === "stroke");
    expect(primitive?.kind === "stroke" && primitive.points).toBeInstanceOf(Float64Array);
    // 続けて読むときは同じ Web Worker を使う
    const jww = await reader.read(await fixture("jww/minimal-v600.jww"));
    expect(jww.format).toBe("jww");
  });

  it("読めない図面は Rust のメッセージの Error、上限を超えたら LimitError で拒否する", async () => {
    const { reader } = open();
    await expect(reader.read(new TextEncoder().encode("not a drawing"))).rejects.toThrow(/対応していない形式/);
    const limited = reader.read(await fixture("dxf/r12.dxf"), { limits: { maxEntities: 1 } });
    await expect(limited).rejects.toBeInstanceOf(LimitError);
    await expect(limited).rejects.toMatchObject({ limit: "entities" });
  });

  // WebAssembly は途中で止まらないので、時間を超えた読み取りは Web Worker ごと止め、次の読み取りは作り直した worker で読む
  it("時間の上限を超えたら Web Worker を止めて LimitError で拒否し、次は作り直して読む", async () => {
    const { reader, created } = open();
    // 読むのに数百ミリ秒かかる図面（線分 5 万本の DXF、Node.js で約 250 ミリ秒）。WebAssembly を読み込んだ後でも、上限の 20 ミリ秒を必ず超える
    const lines = Array.from({ length: 50_000 }, (_, i) => `0\nLINE\n8\n0\n10\n${i}\n20\n0\n11\n${i}\n21\n1\n`);
    const large = new TextEncoder().encode(`0\nSECTION\n2\nENTITIES\n${lines.join("")}0\nENDSEC\n0\nEOF\n`);
    await reader.read(await fixture("dxf/r12.dxf"));
    // 20 ms の読み取りを始める前に取得する。途中で fetch を待つと、拒否を検証する前に unhandledrejection になる。
    const nextBytes = await fixture("jww/minimal-v600.jww");
    const slow = reader.read(large, { limits: { timeout: 20 } });
    const next = reader.read(nextBytes);
    await expect(slow).rejects.toMatchObject({ name: "LimitError", limit: "time" });
    await expect(slow).rejects.toThrow("読み取りが上限の 0.02 秒を超えたため、途中でやめました");
    expect((await next).format).toBe("jww");
    expect(created()).toBe(2);
  });

  it("signal でやめると、待っている読み取りは外し、読んでいる読み取りは Web Worker を止める", async () => {
    const { reader, created } = open();
    const bytes = await fixture("dwg/r2018.dwg");
    const reading = new AbortController();
    const waiting = new AbortController();
    const first = reader.read(bytes, { signal: reading.signal });
    const second = reader.read(bytes, { signal: waiting.signal });
    const third = reader.read(bytes);
    waiting.abort(new Error("待っている読み取りをやめた"));
    await expect(second).rejects.toThrow("待っている読み取りをやめた");
    // 1 つ目は読んでいる途中（Web Worker が WebAssembly を読み込んでいるところ）でやめる
    reading.abort(new Error("読んでいる読み取りをやめた"));
    await expect(first).rejects.toThrow("読んでいる読み取りをやめた");
    expect((await third).format).toBe("dwg");
    expect(created()).toBe(2);
    // やめた後の signal では読まない
    await expect(reader.read(bytes, { signal: reading.signal })).rejects.toThrow("読んでいる読み取りをやめた");
  });

  it("terminate の後は、待っている読み取りもこれからの読み取りも拒否する", async () => {
    const { reader } = open();
    const bytes = await fixture("dxf/r12.dxf");
    const pending = reader.read(bytes);
    reader.terminate();
    await expect(pending).rejects.toThrow(/terminate|止めました/);
    await expect(reader.read(bytes)).rejects.toThrow(/terminate/);
  });

  it("WebAssembly を読み込めなかったら、その読み取りを拒否し、次は Web Worker を作り直す", async () => {
    let created = 0;
    // WebAssembly の印（\0asm）だけで、版も中身もないモジュール
    const reader = new ReaderWorker(
      () => {
        created++;
        return createWorker();
      },
      { wasm: "data:application/wasm;base64,AGFzbQ==" },
    );
    workers.push(reader);
    const bytes = await fixture("dxf/r12.dxf");
    await expect(reader.read(bytes)).rejects.toThrow();
    await expect(reader.read(bytes)).rejects.toThrow();
    expect(created).toBe(2);
  });
});
