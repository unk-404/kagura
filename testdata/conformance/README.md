# Vendored conformance smoke corpus

This directory contains a small, redistributable smoke corpus copied from
the upstream suites. The full suites stay out of the repository; CI fetches
pinned revisions separately.

- `test262/`: <https://github.com/tc39/test262>, BSD-3-Clause; see
  `test262/LICENSE`. Files retain their upstream copyright headers.
- TypeScript conformance will be added under `typescript/` when the
  TypeScript runtime/diagnostic runner exists.

These files are test inputs only; they are not a claim of full conformance.
