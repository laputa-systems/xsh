#!/bin/xsh
use lib.gnu

type Options = {directory: Str, input: Str?, strip: Str, quiet: Bool, force: Bool, unified: Bool, help: Bool, version: Bool, files: List[Str]}

# A named original maps both headers to that file. Unified hunk content stays
# byte-for-byte unchanged, and the native root rejects symlink/path escapes.
pure retarget(text: Str, name: Str) -> Str {
  var output = ""
  for line in text.lines() {
    if line.starts_with("--- ") { output += f"--- {name}\n" } else if line.starts_with("+++ ") { output += f"+++ {name}\n" } else { output += line + "\n" }
  }
  output
}

proc main(...argv: List[Str]) [fs, io, process, env, error] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2},
    directory: {form: "-d --directory DIR", default: "."},
    input: {form: "-i --input PATCHFILE"},
    strip: {form: "-p --strip NUM", default: "0"},
    quiet: {form: "-s --quiet --silent", default: false},
    force: {form: "-f --force", default: false},
    batch: {form: "-t --batch", unsupported: true},
    unified: {form: "-u --unified", default: false},
    dry: {form: "--dry-run", unsupported: true},
    reverse: {form: "-R --reverse", unsupported: true},
    fuzz: {form: "-F --fuzz NUM", unsupported: true},
    backup: {form: "-b --backup", unsupported: true},
    forward: {form: "-N --forward", unsupported: true},
    context: {form: "-c --context", unsupported: true},
    normal: {form: "-n --normal", unsupported: true},
    output: {form: "-o --output FILE", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: patch [OPTION]... [ORIGINALFILE [PATCHFILE]]\nApply unified or Git text patches through a rooted filesystem operation.\nFiles apply in order; later failure keeps earlier changes.\n-d DIR  change application directory\n-i FILE  read patch from FILE (default stdin)\n-p NUM  strip path components\n-s  suppress success messages\n-f  allow replacing existing outputs; mismatched hunks fail\n-u  require unified patches"); return }
  if opts.version { gnu.version("patch"); return }
  if opts.files.len() > 2 { gnu.extra_operand(opts.files[2], 2) }
  let components = opts.strip.parse_int() ?? -1
  if components < 0 { gnu.usage_error("invalid strip count", 2) }
  if opts.input != null and opts.files.len() > 1 { gnu.usage_error("patch input specified twice", 2) }
  let input = opts.input ?? opts.files.get(1) ?? "-"
  let loaded = if input == "-" { io.stdin_text() } else { fp"{input}".read_text() }
  if let Err(failure) = loaded { gnu.name_error(input, failure); exit 2 }
  var text = loaded?
  var has_hunks = false
  for line in text.lines() { if line.starts_with("@@ ") { has_hunks = true } }
  if opts.unified and !has_hunks { gnu.error("input is not a unified patch"); exit 2 }
  if !has_hunks { gnu.error("only unified text patches with hunks are supported"); exit 2 }
  var root = fp"{opts.directory}"
  var strip = components
  if let Ok(original) = opts.files.get(0) {
    if text.starts_with("diff --git ") { gnu.error("a named original cannot override Git patch metadata"); exit 2 }
    let target = if original.starts_with("/") { fp"{original}" } else { fp"{root}/{original}" }
    root = target.parent()
    text = retarget(text, target.basename())
    strip = 0
  }
  let result = patch.apply(root, text, strip_components: strip, overwrite: opts.force)
  match result {
    Ok(applied) => { if !opts.quiet { gnu.write_text(f"patched {applied.files} {if applied.files == 1 { "file" } else { "files" }}\n") } }
    Err(failure) => { gnu.error(gnu.strerror(failure)); exit 1 }
  }
}
