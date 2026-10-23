import { createReadStream, existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, extname, join, resolve } from "node:path";
import { defaultClientConditions, defineConfig, type Plugin } from "vite";
import { port } from "./scripts/local-ports.mjs";

/**
 * 試用画面で、Git に入れない図面を ?sample=名前 で開くための配信。
 * ZUMEN_SAMPLES に図面を置いたフォルダーを指定したときだけ有効で、フォルダーの直下のファイル名だけを受け付ける。
 */
function samples(): Plugin {
  const directory = process.env.ZUMEN_SAMPLES ? resolve(process.env.ZUMEN_SAMPLES) : null;
  return {
    name: "zumen-samples",
    configureServer(server) {
      if (!directory) return;
      server.middlewares.use("/samples/", (request, response, next) => {
        const name = decodeURIComponent((request.url ?? "").replace(/^\//, "").split("?")[0] ?? "");
        const file = join(directory, basename(name));
        if (!name || basename(name) !== name || !existsSync(file) || !statSync(file).isFile()) {
          next();
          return;
        }
        response.setHeader("Content-Type", "application/octet-stream");
        createReadStream(file).pipe(response);
      });
    },
  };
}

/**
 * pdf.js が URL から読むファイルを、試用画面の pdfjs/ に置く（開発中は配り、ビルドでは出力に含める）。
 * cmaps は図面が埋め込まない日本語などのフォントの文字、wasm はスキャンした図面の JPEG 2000・JBIG2 の画像と
 * ICC のための復号器（どちらもフォルダーに各ファイルのライセンスがある）。PDF の中のスクリプトは動かさないので
 * quickjs は除く。standard_fonts は GPL-2.0（フォントの例外つき）の Liberation のフォントを含むので配らず、
 * pdf.js は埋め込まれていない標準の 14 書体をシステムのフォントで描く（docs/architecture.md の npm dependencies）
 */
function pdfjsFiles(): Plugin {
  const root = dirname(createRequire(import.meta.url).resolve("pdfjs-dist/package.json"));
  const folders = ["cmaps", "wasm"];
  const served = (folder: string, name: string) =>
    folders.includes(folder) &&
    !name.startsWith("quickjs") &&
    existsSync(join(root, folder, name)) &&
    statSync(join(root, folder, name)).isFile();
  const types: Record<string, string> = { ".wasm": "application/wasm", ".js": "text/javascript" };
  return {
    name: "zumen-pdfjs-files",
    configureServer(server) {
      server.middlewares.use("/pdfjs/", (request, response, next) => {
        const [folder = "", name = "", ...rest] =
          (request.url ?? "").replace(/^\//, "").split("?")[0]?.split("/") ?? [];
        if (rest.length > 0 || basename(name) !== name || !served(folder, name)) {
          next();
          return;
        }
        response.setHeader("Content-Type", types[extname(name)] ?? "application/octet-stream");
        createReadStream(join(root, folder, name)).pipe(response);
      });
    },
    generateBundle() {
      for (const folder of folders) {
        for (const name of readdirSync(join(root, folder))) {
          if (!served(folder, name)) continue;
          this.emitFile({
            type: "asset",
            fileName: `pdfjs/${folder}/${name}`,
            source: readFileSync(join(root, folder, name)),
          });
        }
      }
    },
  };
}

const serverPort = Number(port("playground", process.env.PORT));

export default defineConfig({
  root: "playground",
  // 相対パスにして、GitHub Pages のようなサブパスでも試用画面を開けるようにする
  base: "./",
  // パッケージの src を直接読み、変更をすぐ反映する（package.json の exports の @zumenjs/source）
  resolve: { conditions: ["@zumenjs/source", ...defaultClientConditions] },
  server: { host: "127.0.0.1", port: serverPort, strictPort: true },
  preview: { host: "127.0.0.1", port: serverPort, strictPort: true },
  plugins: [samples(), pdfjsFiles()],
  build: {
    rolldownOptions: {
      // "use client" は Next.js で使うときの指定。試用画面のバンドルでは意味を持たないので警告を出さない
      onwarn(warning, warn) {
        if (warning.code !== "MODULE_LEVEL_DIRECTIVE") warn(warning);
      },
    },
  },
});
