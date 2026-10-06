#!/bin/xsh
use lib.gnu
use lib.file_magic

type Options = {brief: Bool, mime: Bool, mime_type: Bool, mime_encoding: Bool, follow: Bool, no_follow: Bool, zero: Bool, separator: Str, list: Str?, error: Bool, help: Bool, version: Bool, files: List[Str]}
type Identification = {description: Str, mime: Str, encoding: Str}

proc identify(name: Str, follow: Bool) [fs, io, error] -> Result[Identification] {
  if name == "-" {
    var sample = b""
    while sample.len() < 8192 {
      let chunk = io.stdin_read(8192 - sample.len())?
      if chunk.len() == 0 { break }
      sample = bytes.concat([sample, chunk])
    }
    return Ok(file_magic.classify(sample))
  }
  let target = fp"{name}"
  let meta = fs.stat(target, follow_symlinks: follow)?
  if meta.kind == "dir" { return Ok({description: "directory", mime: "inode/directory", encoding: "binary"}) }
  if meta.kind == "symlink" { return Ok({description: f"symbolic link to {target.readlink()?}", mime: "inode/symlink", encoding: "binary"}) }
  if meta.kind != "file" { return Ok({description: meta.kind, mime: f"inode/{meta.kind}", encoding: "binary"}) }
  let sample = bytes.read_at(target, 0, if meta.size < 8192 { meta.size } else { 8192 })?
  if sample.starts_with(b"\x7fELF") {
    let info = elf.inspect(target)?
    let endian = if info.endian == "little" { "LSB" } else { "MSB" }
    let kind = if info.type == "shared" { "shared object" } else if info.type == "relocatable" { "relocatable" } else if info.type == "core" { "core file" } else { "executable" }
    let linkage = if info.interpreter != "" or !info.needed.is_empty() { "dynamically linked" } else { "statically linked" }
    return Ok({description: f"{info.class} {endian} {kind}, {info.machine}, {linkage}", mime: if info.type == "shared" { "application/x-sharedlib" } else { "application/x-executable" }, encoding: "binary"})
  }
  Ok(file_magic.classify(sample))
}

proc main(...argv: List[Str]) [fs, io, error, process, env] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    brief: {form: "-b --brief", default: false},
    mime: {form: "-i --mime", default: false},
    mime_type: {form: "--mime-type", default: false},
    mime_encoding: {form: "--mime-encoding", default: false},
    follow: {form: "-L --dereference", default: false},
    no_follow: {form: "-h --no-dereference", default: false},
    zero: {form: "-0 --print0", default: false},
    separator: {form: "-F --separator STRING", default: ":"},
    list: {form: "-f --files-from FILE"},
    error: {form: "-E --error", default: false},
    magic: {form: "-m --magic-file FILE", unsupported: true},
    decompress: {form: "-z --uncompress", unsupported: true},
    special: {form: "-s --special-files", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: file [OPTION]... [FILE]...\nIdentify files by deterministic signatures and text encodings.\n  -b, --brief  omit filename\n  -i, --mime  MIME type and encoding\n  --mime-type  type only\n  --mime-encoding  encoding only\n  -L, --dereference  follow symbolic links\n  -f, --files-from FILE\n  -F, --separator STRING\n  -0, --print0  NUL after filename\n  -E, --error  fail for inaccessible files"); return }
  if opts.version { gnu.version("file"); return }
  var files = opts.files
  if let list = opts.list {
    let text = if list == "-" { io.stdin_text()? } else { fp"{list}".read_text()? }
    files = text.lines().collect().extend(files)
  }
  if files.is_empty() { gnu.missing_operand() }
  var failed = false
  for name in files {
    let result = identify(name, opts.follow and !opts.no_follow)
    let prefix = if opts.brief { "" } else { name + (if opts.zero { "\0" } else { "" }) + opts.separator + " " }
    match result {
      Ok(info) => {
        let text = if opts.mime or (opts.mime_type and opts.mime_encoding) { f"{info.mime}; charset={info.encoding}" } else if opts.mime_type { info.mime } else if opts.mime_encoding { info.encoding } else { info.description }
        gnu.write_text(prefix + text + "\n")
      }
      Err(failure) => {
        gnu.write_text(prefix + f"cannot open {gnu.quote(name)} ({gnu.strerror(failure)})\n")
        if opts.error { failed = true }
      }
    }
  }
  if failed { exit 1 }
}
