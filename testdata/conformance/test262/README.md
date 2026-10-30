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
- `--filter SUBSTRING` selects discovered cases whose path contains SUBSTRING.
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

## P1 support

The current P1 slice adds:

- `new` with `Error`, `TypeError`, `RangeError`, `ReferenceError`,
  `SyntaxError`, `Test262Error`, `Object`, and `Array` targets
- `JSON.parse` backed by `serde_json`; `JSON.stringify` uses the existing
  object/array serializer
- array/object destructuring declarations (`var [a,b]`, `var {x,y}`),
  including nested, renamed, default-valued, array rest, object rest, and
  destructuring parameters
- object spread in object literals (`{ ...source }`) and array literals
  (`[...source]`)
- `globalThis` global object and stable script `this`
- stable builtin identity through a per-interpreter builtin cache
- logical assignment (`&&=`, `||=`, `??=`), nullish coalescing, and fixed
  conditional-expression stack handling
- exponentiation, numeric separators, and `Array.prototype.findLast` /
  `findLastIndex`
- `Function` global/prototype surface (`call`/`apply`/`bind` properties); the
  dynamic `Function` constructor body is not implemented yet
- test262 `assert` object shim for includes such as `propertyHelper.js`
- `String.fromCharCode`, `String.fromCodePoint`, and Boolean primitive
  `toString`/`valueOf`
- Number prototype formatting (`toString` radix, `toFixed`, `toExponential`,
  `toPrecision`) and `Number.isNaN/isFinite/isInteger/isSafeInteger`
- global `NaN` and `Infinity`
- `Array.prototype.copyWithin`, `flat`, `flatMap`, and `reduceRight`
- callable `Object(value)` primitives and object wrappers, plus
  `toLocaleString` on the object prototype
- sloppy duplicate parameters and strict duplicate-parameter rejection
- strict-mode early errors for `eval`/`arguments` bindings and `delete`
  of an unqualified identifier
- `String` primitive wrappers and `padStart` / `padEnd` / `codePointAt` /
  `substr` / `at` / `concat` / `localeCompare`
- `Function.prototype.call` / `apply` and non-enumerable builtin prototype
  methods
- duplicate `var` declarations and `Array.prototype`/`Object.prototype` lookup
- `Array.prototype.findIndex`, `fill`, `at`, and `lastIndexOf`
- canonical array indices are distinguished from named properties, so
  `a[4294967295]` does not try to grow the elements store
- `Date` construction, `Date.now` / `Date.parse` / `Date.UTC`, UTC getters,
  `getTime`, `valueOf`, and `toISOString` (via `chrono`; local-time getters are
  currently UTC)
- `Object.prototype.toString.call` and `Object.prototype.hasOwnProperty.call`
- `Math` basics plus `hypot`, `imul`, `clz32`, `cbrt`, `fround`, `log2`,
  `log10`, `acos`, `asin`, `atan`, `atan2`, `sin`, `cos`, `tan`, `exp`
- `--filter` selects a path substring before applying `--limit`
- `var` declarations, prefix/postfix `++`/`--`, `instanceof`, `for...in`, `switch`
- object computed access (`object[key]`) for string keys
- constructor values for the error families, loaded as builtin globals
- `assert.throws`, `assert.compareArray`, and `assert.deepEqual`
- loading and executing extra test262 harness includes
- conditional expressions (`cond ? then : else`)
- `onlyStrict` execution via a prepended `"use strict"` directive
- user-function `new` construction and generic `new` on resolved values
- undeclared assignments throw a runtime `ReferenceError`
- a supported-feature whitelist instead of skipping every non-empty feature list
- global `String`/`Number`/`Boolean`/`parseInt`/`parseFloat`/`isNaN`/`isFinite`
- basic `Math` methods and constants, `JSON.stringify`, `typeof` of unresolved names

These changes do not mean the skipped features are implemented; they just stop
hiding runnable cases behind harness gaps.

## Why skips are expected

A test262 runner normally skips cases that require unsupported flags or
features. Here the skip set is large because this engine is still an M1 subset:
`Temporal`, destructuring, classes, generators/async, TypedArrays, `Symbol`,
`BigInt`, and `Proxy` are not implemented. Extra harness includes are loaded
and executed; if the engine cannot parse them, the case fails rather than being
silently skipped. Skipped cases are counted as `skip`, never as `pass`. A high
skip count is normal for an in-progress engine, but it is **not** a claim of full
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

## Report baseline

Local full-corpus run on 2026-10-30 against pinned test262
`91c70759363c46557b694792bf34a2551a7af5ba`, Node 22.14.0:

```text
54029 selected, 21572 executed, 4622 passed, 16950 failed, 32457 skipped
oracle node: 4622 checked, 4617 matched, 5 mismatched
```

The five Node mismatches are tracked as differential warnings, not hidden:

- Annex-B function-declaration case where Node 22 evaluates `f()` as `2`
  while test262 asserts `1`;
- two `eval-code/direct` cases where the Node harness reports failure while
  Kagura completes;
- `assign-to-global-undefined`, where the Node vm context accepts an
  assignment that test262 treats as an error;
- one staging `regress-633741` case.

They are counted as mismatches, not as passes.

The full report is informational while the engine is pre-0.0.1; the gated
suite remains `cargo test --workspace`.
