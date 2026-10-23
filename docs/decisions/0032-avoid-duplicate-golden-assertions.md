# 0032: Let golden tests own fixture scene values

| Item | Value |
| --- | --- |
| Date | 2026-10-24 |
| Status | Accepted |

## Context

The TypeScript format tests compare complete fixture scenes with checked-in golden files. Several unit tests also
read the same fixtures to check a subset of those scene values, repeating both parsing and expected data maintenance.

## Options

- Keep the repeated assertions for a second description of the expected values.
- Use golden comparisons for fixture scene values and retain tests with an independent purpose.

## Decision

Remove tests that only repeat fixture scene values already checked by golden comparisons. Keep measurements,
geometric invariants such as solid areas, parser model assertions, changed inputs, binding and browser behavior,
cross-language parity, property contracts and the independent entity ID record.

For underlay JSON, retain comparisons in both directions and the optional-field case; remove assertions already
covered by those comparisons and the test that merely repeats the list of kind constants.

## Consequences

There are fewer duplicate expectations and fixture reads. Golden updates still need review of geometry and notices;
regenerating the files does not establish that the new output is correct. This change leaves golden files, fixture
bytes, production code and the verification commands unchanged.

## References

- [Golden comparisons](../../tests/golden.ts)
- [Entity IDs](../architecture.md#entity-ids)
