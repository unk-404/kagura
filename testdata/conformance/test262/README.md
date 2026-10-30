# test262

This directory vendors only the small official negative-parse smoke set used by
the default test suite. The full test262 corpus is intentionally not committed:
it is large and is fetched from the official distribution when the full report
is needed.

## Full corpus

```sh
curl -L --fail -o /tmp/test262.tar.gz \
  https://codeload.github.com/tc39/test262/tar.gz/refs/heads/main
mkdir -p /tmp/test262
tar -xzf /tmp/test262.tar.gz -C /tmp/test262 --strip-components=1

cargo run -q -p kagura-cli -- test262 /tmp/test262
```

Useful flags:

- `--limit N` runs only the first N discovered cases.
- `--verbose` prints each case path before execution.
- `--fail-on-error` exits non-zero when any case fails.
- `--min-pass N` exits non-zero when fewer than N cases pass.

Each case has a bounded VM step budget, so a syntax feature that is not
implemented cannot hang the report.

## Report baseline

Local full-corpus run on 2026-10-30 against test262 `main`:

```text
54029 selected, 1391 passed, 11690 failed, 40948 skipped
```

Skipped cases are explicitly categorized:

- unsupported test262 flags (`module`, `async`, `onlyStrict`, `raw`)
- frames beyond `assert.js` / `sta.js`
- any non-empty `features` list
- unsupported assertion helpers (`assert.throws`, `assert.deepEqual`, ...)

The full report is informational while the engine is pre-0.0.1; the gated
suite remains `cargo test --workspace`.
