# 0014: Verify native Windows builds and CAD interoperability

> English reference preserved when Japanese became the primary documentation. See [0014-verify-on-native-windows.md](0014-verify-on-native-windows.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

Linux CI and comparisons between our TypeScript and Rust readers cannot detect Windows executable paths,
MSVC floating-point differences, Python extension packaging, or errors shared by both readers. Jw_cad and
the installed Japanese Windows fonts provide independent checks of the drawings that users actually see.

## Options

1. Keep Linux CI and perform all Windows checks manually.
2. Run the same verification entry point on Linux and Windows, and keep native CAD interoperability checks manual.
3. Require proprietary CAD software and exact platform font pixels in CI.

## Decision

Use option 2. Both CI runners execute `pnpm run check` and install a built Python wheel into a fresh environment.
Scripts use native executable paths and child-process environment variables instead of POSIX shell syntax.
Browser tests check text pixels and hit areas with MS Gothic and MS Mincho font families without requiring
identical rasterization across systems. Windows font availability is recorded in manual verification.

Compare externally stored drawings with Jw_cad on Windows, and turn reproducible rules into synthetic fixtures.
Record the application version, observations and limits in `docs/verification.md`; never commit sample drawings.

## Consequences

CI takes more runner time, but Windows regressions block merging. Native CAD rendering and exact font metrics
remain manual checks. Machines without the symlink privilege still test root containment through junctions;
the privilege-dependent symlink check reports a skip without changing Windows settings.

## References

- [Verification records](../verification.md#windows-native-verification)
- [Contributing](../../CONTRIBUTING.md#windows)
