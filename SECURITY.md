# Security policy

## Supported versions

zumen is pre-1.0. Only the latest release receives fixes.

## Reporting a vulnerability

Please report privately with GitHub's private vulnerability reporting
("Report a vulnerability" on the repository's Security tab). Do not open a public issue.
Do not include confidential drawings; a small file made to reproduce the problem is best.

## What counts

zumen reads drawing files that users open, which may come from anyone. Please report, for example:

- a file that makes a reader hang, crash the page, or use excessive memory or CPU;
- any network access made by the libraries;
- script injection through text stored in a drawing (for example in the React components).

The readers work on bytes in memory, limit how deeply placements and blocks nest, and make no network requests.
Text from drawings is drawn on a canvas or inserted as text, never as HTML.
