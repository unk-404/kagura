# Bytecode compatibility fixtures

`bytecode/v1/*.kbc` are current pre-release bytecode artifacts.
They are **not** a published compatibility promise yet; the format stays v1
until the first runtime release. After 0.0.1 is published, v1 becomes frozen
and future changes require a new version.

They are inputs for reader/executor regression tests; regenerate only while
pre-release.
