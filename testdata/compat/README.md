# Bytecode compatibility fixtures

`bytecode/v1/*.kbc` are frozen artifacts produced by the original M0
compiler. `bytecode/v2/*.kbc` use the v2 `CallHost` FFI opcode.

They are inputs for the permanent reader/executor tests; do not regenerate
or replace existing fixtures. New format versions get a new directory.
