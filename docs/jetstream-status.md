# Kagura benchmark status

This file records what was actually run. It is not a marketing score.

## test262 (P1 integration)

Pinned corpus: `test262-main`, SHA `91c70759363c46557b694792bf34a2551a7af5ba`.
Node oracle: Node.js 22.14.0 on the same machine.

Latest verified run (HEAD `7aa0cbc`):

```text
53733 selected, 22267 executed, 6614 passed, 15653 failed, 31466 skipped
oracle node: 6614 checked, 6599 matched, 15 mismatched
```

The 15 mismatches are reported in the runner output, not hidden. They are
11 `compound-assignment` cases where Node 22's V8 calls `ToPropertyKey` twice,
2 `eval-code/direct` cases, and 2 staging cases.

## JetStream 3.0

Pinned corpus: WebKit/JetStream branch `JetStream3.0`, commit
`06785cf861ac44855f168cbbe829278c2802e6de`.

The official JetStream 3.0 driver has **not** been executed by Kagura. It is
blocked by missing Promise job semantics, dynamic `loadString`/`runString`
loading, Wasm, TypedArray, Workers/SAB/Atomics, and host APIs. No official
score is reported.

A direct-workload subset is available as a diagnostic only:

```text
node tools/jetstream-direct-report.mjs --corpus <JetStream dir> --timeout-ms 15000
```

It writes a Markdown table and always leaves `officialScore: null`.

Latest 11-workload subset:

| Workload | Kagura | Node |
| --- | --- | --- |
| crypto-md5 | ok, 6221 ms | ok, 48 ms |
| crypto-sha1 | ok, 3989 ms | ok, 45 ms |
| n-body | ok, 1837 ms | ok, 41 ms |
| 3d-cube | timeout | ok |
| 3d-raytrace | timeout | ok |
| base64 | fail (second-run return bug) | ok |
| date-format-tofte | fail | ok |
| date-format-xparb | fail | ok |
| regex-dna | fail | ok |
| string-unpack-code | fail | ok |
| tagcloud | fail | ok |

These are direct single-workload runs, not the official harness and not a
comparable official score.

## Verification commands

```text
cargo test --workspace -q
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --target i686-pc-windows-msvc -q
cargo build --workspace --target wasm32-unknown-unknown
```

Current status: 233 workspace tests pass; x86_64 and i686 tests pass; wasm32
builds but is not executed.
