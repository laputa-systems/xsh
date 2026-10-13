# Examples

Run an example from the repository root with `xsh examples/NAME.xsh`.
Examples are complete, small programs that show several XSH modules working
together. For larger systems programs, see
[Showcase](../docs/SHOWCASE.md).

| Program | What it demonstrates |
|---|---|
| [`json.xsh`](json.xsh) | Typed JSON files and JSON Lines records. |
| [`streams.xsh`](streams.xsh) | Filesystem records, sorting, and bounded parallel mapping. |
| [`processes.xsh`](processes.xsh) | Host and user identity, signals, child processes, and command timing. |
| [`release-package.xsh`](release-package.xsh) | Creating and extracting an archive, applying a patch, and compressing a file. |
| [`typed-cli-options.xsh`](typed-cli-options.xsh) | Typed options, repeated values, command dispatch, and regular-expression matching. |

`typed-cli-options.xsh` accepts options after `--`, for example:

```sh
xsh examples/typed-cli-options.xsh -- --root target/demo --jobs=3 \
  --define A=1 --define B=2 --verbose
```
