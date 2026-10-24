# 0016: Send scene JSON from the reader worker and decode it on the page

> English reference preserved when Japanese became the primary documentation. See [0016-send-worker-scenes-as-json.md](0016-send-worker-scenes-as-json.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-21 |
| Status | Accepted |

## Context

`ReaderWorker` keeps format parsing in a Web Worker so that an application can stop a read by terminating the worker.
The worker previously decoded the Rust reader's JSON into a `Scene`, then posted the scene and transferred its point
buffers. Large drawings have many small `Float64Array`s, so preparing and cloning that object graph can dominate the
read. The Chromium measurements in [PR #58](https://github.com/zumenjs/zumen/pull/58) show about 14.5 seconds for this
path on a 227,139-entity SFC, compared with about 2.9 seconds when the worker sends JSON. These are measurements of one
drawing and environment, not latency guarantees.

This record refines the worker handoff described in [decision 0005](0005-browser-packages-for-embedding.md).

## Options

- Decode the scene in the worker and transfer its arrays. This keeps JSON decoding off the page, but retains the
  measured cost of cloning a large object graph and enumerating many transferables.
- Send the JSON string already produced by Rust, then call `sceneFromJson` on the page. This reuses the existing scene
  representation and decoder, avoids the object-graph handoff and requires no new public API or runtime dependency.
- Define a packed binary representation with fewer transferable buffers. This may reduce both copying and decoding,
  but introduces another scene representation and its compatibility and validation work.

## Decision

Use `readJson` in the worker and post its string. `ReaderWorker` calls `sceneFromJson` on receipt and returns the same
`Scene` as synchronous `readScene`. A decoding error rejects the read and allows queued reads to continue.

Keep format parsing in the worker and retain the existing timeout, abort and restart behavior. Defer a binary
representation until measurements justify its additional complexity.

## Consequences

- The page does some synchronous work: the cited large drawing takes about 0.9 seconds to decode there. During that
  step it cannot process input, cancellation or timer callbacks; the worker timeout cannot interrupt page-side JSON
  decoding. Applications should still choose appropriate byte and entity limits.
- The JSON string is copied rather than transferred. This decision reduces the measured handoff time, but does not
  promise zero-copy transport or lower peak memory for every drawing.
- Typed arrays, entity ids, notices, views and other scene data must remain equivalent to `readScene`. Browser tests
  cover typed arrays, limits, cancellation and worker recovery; review checks compare complete serialized scenes for
  real DXF, JWW and SFC drawings and a generated drawing with 50,000 lines.
- The worker protocol is internal. The public `ReaderWorker.read` result remains `Promise<Scene>`.

## References

- [PR #58: measurements and implementation](https://github.com/zumenjs/zumen/pull/58)
- [Reader architecture](../architecture.md#the-readers-in-the-browser)
- [Scene JSON conversion](../../packages/core/src/json.ts)
