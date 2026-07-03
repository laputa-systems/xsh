# XSH Reference

Generated from the docs engine. Do not edit by hand.

This file covers non-stdlib reference data. See `docs/STDLIB.md` for standard modules, value methods, and standard record schemas.

## Machine Index

```xsh-reference
kind: reference
version: 1
sections: stream-stages, run-forms, effects, cli-forms, trace-events, language
```

## Stream Stages

```xsh-reference
stage: where
stage: map
stage: par-map
stage: each
stage: batch
stage: sort
stage: sort-by
stage: take
stage: drop
stage: first
stage: last
stage: unique-by
stage: enumerate
stage: zip
stage: range
stage: repeat
stage: tee
stage: sum
stage: min
stage: max
stage: group-by
stage: fold
stage: reduce
stage: flat-map
stage: any
stage: all
stage: shuffle
stage: table.print
stage: text.lines
stage: bytes.chunks
stage: json.lines
stage: json.stream
stage: count
stage: collect
```

## Run Forms

| Form | Returns | Nonzero Exit | Setup/Spawn/Capture Failure |
|---|---|---|---|
|`run` in statement position|`Unit`|propagates `ProcessError`|propagates `ProcessError`|
|`run` in value position|`Status`|status data|propagates `ProcessError`|
|`run.status`|`Status`|status data|propagates `ProcessError`|
|`run.text`|`Result[Str, ProcessError]`|`Err(ProcessError)`|`Err(ProcessError)`|
|`run.bytes`|`Result[Bytes, ProcessError]`|`Err(ProcessError)`|`Err(ProcessError)`|
|`run.capture --text`|`Result[{status, stdout: Str, stderr: Str}, ProcessError]`|`Ok(record)` with status|`Err(ProcessError)`|
|`run.capture --bytes`|`Result[{status, stdout: Bytes, stderr: Bytes}, ProcessError]`|`Ok(record)` with status|`Err(ProcessError)`|
|`run.stream --text`|`Result[Stream[Str], ProcessError]`|`Err(ProcessError)`|`Err(ProcessError)`|
|`run.stream --bytes`|`Result[Stream[Bytes], ProcessError]`|`Err(ProcessError)`|`Err(ProcessError)`|

```xsh-reference
run: `run` in statement position -> `Unit`
run: `run` in value position -> `Status`
run: `run.status` -> `Status`
run: `run.text` -> `Result[Str, ProcessError]`
run: `run.bytes` -> `Result[Bytes, ProcessError]`
run: `run.capture --text` -> `Result[{status, stdout: Str, stderr: Str}, ProcessError]`
run: `run.capture --bytes` -> `Result[{status, stdout: Bytes, stderr: Bytes}, ProcessError]`
run: `run.stream --text` -> `Result[Stream[Str], ProcessError]`
run: `run.stream --bytes` -> `Result[Stream[Bytes], ProcessError]`
```

## Effects

| Effect | Covers |
|---|---|
|`fs`|`fs.*`, `archive.*`, `diff.*`, `patch.*`, `user.*`, `group.*`, `module.*`|
|`io`|`io.*`, superset of `fs`, superset of `net`, superset of `process`, superset of `env`|
|`net`|`net.*`, `dns.*`|
|`process`|`run`, `spawn`, `wait`, `ProcessHandle.cancel`, effectful `process.*`, `unix.*`, `linux.*`, `applet.*`|
|`env`|`env.*`, `cd`, `system.*`|
|`time`|`time.*`, delayed retry blocks|
|`error`|`?` propagation outside retry attempt blocks|

```xsh-reference
effect: `fs` -> `fs.*`, `archive.*`, `diff.*`, `patch.*`, `user.*`, `group.*`, `module.*`
effect: `io` -> `io.*`, superset of `fs`, superset of `net`, superset of `process`, superset of `env`
effect: `net` -> `net.*`, `dns.*`
effect: `process` -> `run`, `spawn`, `wait`, `ProcessHandle.cancel`, effectful `process.*`, `unix.*`, `linux.*`, `applet.*`
effect: `env` -> `env.*`, `cd`, `system.*`
effect: `time` -> `time.*`, delayed retry blocks
effect: `error` -> `?` propagation outside retry attempt blocks
```

## CLI Forms

```xsh-reference
cli: xsh SCRIPT [ARGS...]
cli: xsh -- SCRIPT ARGS...
cli: xshi
cli: xsht check [--strict] [--summary] [--annotate] [PATH...]
cli: xsht fmt [--check] [FILE...]
cli: xsht lint [--fix] [--runless] [FILE...]
cli: xsht ast SCRIPT
cli: xsht trace [--raw] [--trace-format text|jsonl|flamegraph] [--trace-file FILE] [--syscalls] [--trace-top-syscalls N] SCRIPT [ARGS...]
cli: xsht test [--cov] [OPTIONS] [FILTER]
cli: xsht docs build
cli: xsht docs check
```

`xsht check` performs parse, resolve, type, effect, and compact-runtime
lowerability checks without executing the script.

## Trace Events

```xsh-reference
trace: script.enter
trace: script.exit
trace: proc.enter
trace: proc.exit
trace: pure.enter
trace: pure.exit
trace: core.call
trace: core.result
trace: module.call
trace: module.result
trace: method.call
trace: method.result
trace: run.start
trace: run.end
trace: stream.stage.enter
trace: stream.stage.exit
```

## Core Language

```xsh-reference
language: source-files
language: comments
language: statements
language: bindings
language: procs
language: pure-functions
language: records
language: results
language: postfix-question
language: fallback
language: run
language: captures
language: streams
language: native-tests
language: command-interpolation
language: path-literals
language: glob-literals
language: display-strings
```
