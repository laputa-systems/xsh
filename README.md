# XSH

XSH is a clean-slate systems scripting language for the glue that holds a
Linux userspace together: installers, maintenance jobs, build recipes, service
checks, and init policy. It keeps what the shell got right (processes, pipes,
files, exit statuses, small tools composed well) and drops what it got wrong:
no word splitting, no implicit eval, no `set -e` traps, no scraping columns out
of text. Values are typed, paths are paths, failures are visible in the code,
and a script is checked before its first command runs.

```bash
# Who is listening on 8080? Scrape two tools and hope the columns don't move.
pid=$(ss -ltnp | awk '$4 ~ /:8080$/' | grep -o 'pid=[0-9]*' | cut -d= -f2)
echo "8080 is held by $(ps -o comm= -p "$pid") ($pid)"
```

```xsh
for owner in process.port(8080)? |> where .state == "LISTEN" {
  print f"8080 is held by ${owner.command} (${owner.pid})"
}
```

## Build

```bash
cargo build --release -p xsh --bins -p xsht --bin xsht
export PATH="$PWD/target/release:$PATH"
xsh examples/streams.xsh       # run a script
xsht check examples/           # type-check without running
```

`xsh` runs scripts. `xsht` is the toolchain: `check`, `test`, `fmt`, `lint`,
`trace`, and `api`.

## Learn

- Start with [`docs/user-tour.md`](docs/user-tour.md). Read it end to end
  before writing your first script.
- [`docs/SPEC.md`](docs/SPEC.md) is the language contract.
- `xsht api summary` lists every standard module, method, and record;
  `xsht api api:fs.files` or `xsht api search:timeout` answers the rest.
