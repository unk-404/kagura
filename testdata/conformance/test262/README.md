# test262

This directory vendors only the small official negative-parse smoke set used by
the default test suite. The full test262 corpus is intentionally not committed:
it is large and is fetched from the official distribution when the full report
is needed.

## Full corpus

The corpus is pinned for reproducible reports:

```sh
TEST262_COMMIT=91c70759363c46557b694792bf34a2551a7af5ba
curl -L --fail -o /tmp/test262.tar.gz \
  "https://codeload.github.com/tc39/test262/tar.gz/$TEST262_COMMIT"
mkdir -p /tmp/test262
tar -xzf /tmp/test262.tar.gz -C /tmp/test262 --strip-components=1

cargo run -q -p kagura-cli -- test262 /tmp/test262
```

The Node oracle pins Node 22.14.0 in CI; install the same version locally for
comparable oracle results. On Windows, use `curl.exe` and `tar.exe` with a local
output directory instead of `/tmp`.

Useful flags:

- `--limit N` runs only the first N discovered cases.
- `--verbose` prints each case path before execution.
- `--fail-on-error` exits non-zero when any case or Node oracle check fails.
- `--min-pass N` exits non-zero when fewer than N cases pass.
- `--node` runs every passing case through Node.js and compares the observable
  stdout and completion status.
- `--node-all` also runs failing Kagura cases through Node and classifies
  `kagura_fail_node_pass`, `both_failed`, and other disagreements.
- `--json` emits a machine-readable report instead of the human summary.
- `--harness DIR` points at the test262 `harness/` directory; by default it is
  discovered beside the corpus root.

Each case has a bounded VM step budget, so a syntax feature that is not
implemented cannot hang the report.

## Machine-readable report

`--json` emits one object with:

- `selected`, `executed`, `passed`, `failed`, `skipped`
- `skip_reasons` grouped by reason
- `failures` (first 25)
- `oracle`: `checked`, `matched`, `mismatched`, `both_failed`, and mismatches

CI uploads this JSON as the `test262-report` artifact.

## What the test means

test262 tests are self-asserting: they call `assert.sameValue`/`assert.throws`
and produce no stdout on success. Therefore test262's primary oracle is its own
assertions, not process output. The `--node` mode adds an independent check:

1. Kagura must execute the case without error (or produce the expected
   `negative` error).
2. For every positive case Kagura passes, Node.js must also pass and emit
   exactly the same stdout.
3. For a negative case, both engines must fail.

This catches cases where Kagura's `assert` host or engine semantics disagree
with Node, but it cannot turn a skipped feature into a pass.

## Why skips are expected

A test262 runner normally skips cases that require unsupported flags or
features. Here the skip set is large because this engine is still an M1 subset:
`Temporal`, destructuring, classes, generators/async, TypedArrays, `Symbol`,
`BigInt`, `Proxy`, and non-`assert.js`/`sta.js` harness files are not
implemented. Those cases are counted as `skip`, never as `pass`. A high skip
count is normal for an in-progress engine, but it is **not** a claim of full
test262 conformance.

## GitHub Actions

- **Actions -> ci -> Run workflow**: enable `Run the full test262 corpus`; an
  optional `limit` narrows the run.
- **Actions -> test262 -> Run workflow**: dedicated full-corpus workflow with
  `limit` and `min_pass` inputs.
- Both workflows run with `--node` and upload `test262-report.txt` as an
  artifact.

The full-corpus job requires at least 1200 passing cases by default, to catch
gross regressions without pretending the whole suite is green.

## Differential sample

To keep a full `--node-all` run bounded, the first 5000 discovered cases give
this taxonomy:

```text
selected 5000, executed 2606, passed 54, failed 2552, skipped 2394
oracle checked 2606, matched 53, mismatched 2459, both_failed 94
mismatched split: 2458 kagura-fail/node-pass, 1 kagura-pass/node-fail
```

The dominant bucket is therefore **Kagura fail / Node pass**: missing engine
semantics and built-ins, not Node disagreements. The one `kagura-pass/node-fail`
case is the Annex-B divergence described below.

## Report baseline

Local full-corpus run on 2026-10-30 against pinned test262
`91c70759363c46557b694792bf34a2551a7af5ba`, Node 22.14.0:

```text
54029 selected, 13080 executed, 1390 passed, 11690 failed, 40949 skipped
oracle node: 1390 checked, 1389 matched, 1 mismatched
```

The single Node mismatch is an Annex-B function-declaration test where Node 22
evaluates `f()` as `2` while the test asserts `1`; Kagura follows the test
assertion. It is reported as a differential warning, not hidden.

The full report is informational while the engine is pre-0.0.1; the gated
suite remains `cargo test --workspace`.
