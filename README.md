# XSH

XSH is a systems scripting language for installers, maintenance jobs, build
recipes, and service checks. It has typed values and paths, explicit error
handling, process pipelines, and structured host data. Scripts are checked
before execution.

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
For an interactive shell, install the matching `xshi` binary and run `xshi`.

```bash
cat > hello.xsh <<'EOF'
print "hello from xsh"
EOF
xsh hello.xsh                 # prints: hello from xsh
xsht check hello.xsh          # type-check without running
```

`xsh` runs scripts. `xsht` provides `check`, `test`, `fmt`, `lint`, `trace`,
and `api`; run `xsht help` for the command list and `xsht help COMMAND` for
command options.

Tool configuration lives in `xsht-config.ini`; see
[configuration](docs/XSHT.md#configuration).

## Learn

- Start with the [user tour](docs/user-tour.md).
- [`docs/SPEC.md`](docs/SPEC.md) is the language contract.
- `xsht api summary` lists every standard module, method, and record;
  `xsht api api:fs.files` or `xsht api search:timeout` answers the rest.

## License

This repository does not currently include a license file.
