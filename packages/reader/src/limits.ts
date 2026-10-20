/** 読み取りの上限。受け取った図面を読むときに、かけてよい大きさを決める */
export interface ReadLimits {
  /** 図面のバイト数（圧縮した SXF の P2Z・SFZ は、展開した後のバイト数） */
  maxBytes?: number;
  /** 表示用の図面の図形の数 */
  maxEntities?: number;
  /**
   * 読み取りにかける時間（ミリ秒）。WebAssembly は読んでいる途中で時刻を測れないので、ReaderWorker だけが使う
   * （時間を超えたら Web Worker を止める）。readScene と reader は無視する
   */
  timeout?: number;
}

/** 超えた上限の種類 */
export type Limit = "bytes" | "entities" | "time";

/** 読み取りの上限を超えたので、読むのをやめた。message は利用者に見せる文 */
export class LimitError extends Error {
  override readonly name = "LimitError";
  constructor(
    message: string,
    /** 超えた上限 */
    readonly limit: Limit,
  ) {
    super(message);
  }
}
