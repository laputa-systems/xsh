#!/bin/xsh
use lib.gnu

type Options = {data: Bool, filesystem: Bool, help: Bool, version: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    data: {form: "-d --data", default: false},
    filesystem: {form: "-f --file-system", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: sync [OPTION] [FILE]...\nSynchronize cached writes to persistent storage.\n  -d, --data         sync only file data\n  -f, --file-system  sync the filesystem containing each file\n"); return }
  if opts.version { gnu.version("sync"); return }
  if opts.data and opts.filesystem { gnu.usage_error("cannot specify both --data and --file-system") }
  if opts.data and opts.paths.is_empty() { gnu.error("--data needs at least one argument"); exit 1 }
  if opts.paths.is_empty() { fs.sync(); return }
  var failed = false
  let mode = if opts.data { "data" } else if opts.filesystem { "filesystem" } else { "all" }
  for name in opts.paths {
    if let Err(failure) = fs.sync_path(fp"{name}", mode: mode) {
      let verb = if (failure.errno ?? 0) in [2, 13, 20] { "opening" } else { "syncing" }
      gnu.error(f"error {verb} {gnu.quote(name)}: {gnu.strerror(failure)}")
      failed = true
    }
  }
  if failed { exit 1 }
}
