#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

pure file_type_name(kind: Str) -> Str {
  match kind {
    "dir" => "directory"
    "file" => "regular file"
    "symlink" => "symbolic link"
    _ => kind
  }
}

pure has_bit(mode: Int, bit: Int) -> Bool {
  return mode / bit % 2 == 1
}

pure mode_octal(mode: Int) -> Str {
  let bits = mode % 512
  let user_bits = bits / 64
  let group_bits = bits / 8 % 8
  let other_bits = bits % 8
  return f"${user_bits}${group_bits}${other_bits}"
}

pure mode_triplet(mode: Int, read_bit: Int, write_bit: Int, exec_bit: Int) -> Str {
  let r = if has_bit(mode, read_bit) { "r" } else { "-" }
  let w = if has_bit(mode, write_bit) { "w" } else { "-" }
  let x = if has_bit(mode, exec_bit) { "x" } else { "-" }
  return f"${r}${w}${x}"
}

pure mode_string(kind: Str, mode: Int) -> Str {
  let file_type = if kind == "dir" { "d" } else if kind == "symlink" { "l" } else { "-" }

  return f"${file_type}${mode_triplet(mode, 0o400, 0o200, 0o100)}${mode_triplet(mode, 0o40, 0o20, 0o10)}${mode_triplet(
    mode,
    0o4,
    0o2,
    0o1,
  )}"
}

proc render_format(fmt: Str, target: Path, meta: FsEntry) [fs, error] -> Str {
  var owner = f"${meta.uid}"
  var owner_group = f"${meta.gid}"

  match user.by_uid(meta.uid) {
    Ok(found_user) => owner = found_user.name
    Err(_) => {}
  }

  match group.by_gid(meta.gid) {
    Ok(found_group) => owner_group = found_group.name
    Err(_) => {}
  }

  var out = fmt
  out = out.replace("%s", f"${meta.size}")
  out = out.replace("%b", f"${meta.blocks_512}")
  out = out.replace("%B", "512")
  out = out.replace("%a", mode_octal(meta.mode))
  out = out.replace("%A", mode_string(meta.kind, meta.mode))
  out = out.replace("%u", f"${meta.uid}")
  out = out.replace("%g", f"${meta.gid}")
  out = out.replace("%U", owner)
  out = out.replace("%G", owner_group)
  out = out.replace("%X", f"${meta.accessed}")
  out = out.replace("%Y", f"${meta.modified}")
  out = out.replace("%F", file_type_name(meta.kind))
  out = out.replace("%n", target.display())
  out = out.replace("%N", f"'${target.display()}'")
  return out
}

proc main(...argv: List[Str]) [fs, error] {
  var fmt = ""
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-c" or arg == "--format" {
      if index + 1 >= argv.len() {
        return Err(usage_error("stat", "-c FORMAT PATH..."))
      }

      fmt = argv[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--format=") {
      fmt = arg.replace("--format=", "")
    } else if arg.starts_with("-c") and arg.count_chars() > 2 {
      fmt = arg.replace("-c", "")
    } else {
      if arg.starts_with("-") {
        return Err(reject_unsupported("stat", arg))
      }

      paths = paths.push(arg)
    }

    index += 1
  }

  if paths.len() == 0 {
    return Err(usage_error("stat", "[-c FORMAT] PATH..."))
  }

  for item in paths {
    let target = fp"${item}"
    let meta = target.metadata()?

    if fmt != "" {
      print render_format(fmt, target, meta)
    } else {
      print f"kind ${meta.kind}"
      print f"size ${meta.size}"
      print f"mode ${meta.mode}"
      print f"uid ${meta.uid}"
      print f"gid ${meta.gid}"
      print f"path ${meta.path}"
    }
  }
}
