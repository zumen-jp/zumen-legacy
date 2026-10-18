import { createReadStream, existsSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";
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

const serverPort = Number(port("playground", process.env.PORT));

export default defineConfig({
  root: "playground",
  // 相対パスにして、GitHub Pages のようなサブパスでも試用画面を開けるようにする
  base: "./",
  // パッケージの src を直接読み、変更をすぐ反映する（package.json の exports の @zumenjs/source）
  resolve: { conditions: ["@zumenjs/source", ...defaultClientConditions] },
  server: { host: "127.0.0.1", port: serverPort, strictPort: true },
  preview: { host: "127.0.0.1", port: serverPort, strictPort: true },
  plugins: [samples()],
  build: {
    rolldownOptions: {
      // "use client" は Next.js で使うときの指定。試用画面のバンドルでは意味を持たないので警告を出さない
      onwarn(warning, warn) {
        if (warning.code !== "MODULE_LEVEL_DIRECTIVE") warn(warning);
      },
    },
  },
});
