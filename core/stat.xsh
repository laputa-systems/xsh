#!/bin/xsh
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure file_type_name(kind: Str) -> Str {
  match kind {
    "dir" => "directory"
    "file" => "regular file"
    "symlink" => "symbolic link"
    else => kind
  }
}

pure has_bit(mode: Int, bit: Int) -> Bool {
  mode / bit % 2 == 1
}

pure mode_octal(mode: Int) -> Str {
  let bits = mode % 512
  let user_bits = bits / 64
  let group_bits = bits / 8 % 8
  let other_bits = bits % 8
  f"{user_bits}{group_bits}{other_bits}"
}

pure mode_triplet(mode: Int, read_bit: Int, write_bit: Int, exec_bit: Int) -> Str {
  let r = if has_bit(mode, read_bit) { "r" } else { "-" }
  let w = if has_bit(mode, write_bit) { "w" } else { "-" }
  let x = if has_bit(mode, exec_bit) { "x" } else { "-" }
  f"{r}{w}{x}"
}

pure mode_string(kind: Str, mode: Int) -> Str {
  let file_type = if kind == "dir" { "d" } else if kind == "symlink" { "l" } else { "-" }

  f"{file_type}{mode_triplet(mode, 0o400, 0o200, 0o100)}{mode_triplet(mode, 0o40, 0o20, 0o10)}{mode_triplet(mode, 0o4, 0o2, 0o1)}"
}

proc render_format(fmt: Str, target: Path, meta: FsEntry) [fs, error] -> Str {
  var owner = f"{meta.uid}"
  var owner_group = f"{meta.gid}"

  if let Ok(found_user) = user.by_uid(meta.uid) {
    owner = found_user.name
  }

  if let Ok(found_group) = group.by_gid(meta.gid) {
    owner_group = found_group.name
  }

  var out = fmt
  out = out.replace("%s", with: f"{meta.size}")
  out = out.replace("%b", with: f"{meta.blocks_512}")
  out = out.replace("%B", with: "512")
  out = out.replace("%a", with: mode_octal(meta.mode))
  out = out.replace("%A", with: mode_string(meta.kind, meta.mode))
  out = out.replace("%u", with: f"{meta.uid}")
  out = out.replace("%g", with: f"{meta.gid}")
  out = out.replace("%U", with: owner)
  out = out.replace("%G", with: owner_group)
  out = out.replace("%X", with: f"{meta.accessed}")
  out = out.replace("%Y", with: f"{meta.modified}")
  out = out.replace("%F", with: file_type_name(meta.kind))
  out = out.replace("%n", with: target.display())
  out = out.replace("%N", with: f"'{target}'")
  out
}

type StatOptions = {format: Str, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: StatOptions = cli.applet(
    argv,
    {
      format: {
        form: "-c --format FORMAT",
        default: "",
      },
      paths: {
        form: "...PATH",
      },
    },
  )?
  let {format: fmt, paths, ..} = opts

  return Err(usage_error("stat", "[-c FORMAT] PATH...")) when paths.is_empty()

  for item in paths {
    let target = fp"{item}"
    let meta = target.metadata()?

    if fmt != "" {
      print render_format(fmt, target, meta)
    } else {
      print f"kind {meta.kind}"
      print f"size {meta.size}"
      print f"mode {meta.mode}"
      print f"uid {meta.uid}"
      print f"gid {meta.gid}"
      print f"path {meta.path}"
    }
  }
}
