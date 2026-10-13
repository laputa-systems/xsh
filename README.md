# XSH

XSH is a clean-slate systems scripting language for the glue that holds a
Linux userspace together: installers, maintenance jobs, build recipes, service
checks, and init policy. It keeps what the shell got right (processes, pipes,
files, exit statuses, small tools composed well) and drops what it got wrong:
no word splitting, no implicit eval, no `set -e` traps, no scraping columns out
of text. Values are typed, paths are paths, failures are visible in the code,
and a script is checked before its first command runs.

```xsh
for owner in process.port(8080)? |> where .state == "LISTEN" {
  print f"8080 is held by {owner.command} ({owner.pid})"
}
```

## Install and run

Choose the `xsh` and `xsht` binaries for your platform from the
[release assets](https://github.com/laputa-systems/xsh/releases), rename them
to `xsh` and `xsht`, and put both in a directory on your `PATH`.
Make them executable with `chmod +x xsh xsht`.

```bash
xsh examples/streams.xsh       # run a script
xsht check examples/           # type-check without running
```

`xsh` runs scripts. `xsht` provides `check`, `test`, `fmt`, `lint`, `trace`,
and `api`; run `xsht help` for the command list and `xsht help COMMAND` for
command options.

Tool configuration lives in `xsht-config.ini`; see
[configuration](docs/XSHT.md#configuration).

## Learn

- Start with [`docs/user-tour.md`](docs/user-tour.md). Read it end to end
  before writing your first script.
- [`docs/SPEC.md`](docs/SPEC.md) is the language contract.
- `xsht api summary` lists every standard module, method, and record;
  `xsht api api:fs.files` or `xsht api search:timeout` answers the rest.
