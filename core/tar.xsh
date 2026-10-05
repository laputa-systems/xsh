#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  AppletError.Usage(f"{applet_name}: unsupported option '{flag}'")
}

pure common_int(raw: Str, label: Str) -> Result[Int] {
  match raw {
    "1k" | "1K" => 1024
    _ => raw.parse_int().context("usage", f"unsupported {label} '{raw}'")?
  }
}

proc main(...argv: List[Str]) [fs, error] {
  var mode = ""
  var archive_path = p""
  var root = p"."
  var compression = "auto"
  var strip = 0
  var overwrite = false
  var operands = []

  for token in cli.tokens(argv, ["f", "C", "strip-components"])? {
    match token.kind {
      "short" => {
        match token.name {
          "c" => mode = "create"
          "t" => mode = "list"
          "x" => mode = "extract"
          "z" => compression = "gz"
          "j" => compression = "bz2"
          "J" => compression = "xz"
          "f" => archive_path = fp"{token.value}"
          "C" => root = fp"{token.value}"
          _ => return Err(reject_unsupported("tar", f"-{token.name}"))
        }
      }
      "long" => {
        match token.name {
          "overwrite" => overwrite = true
          "strip-components" => strip = common_int(token.value, "strip components")?
          _ => return Err(reject_unsupported("tar", f"--{token.name}"))
        }
      }
      "operand" => operands += [fp"{token.name}"]
      _ => {}
    }
  }

  if mode == "create" {
    return Err(usage_error("tar", "expected entries to archive")) when operands.len() == 0

    archive.tar_create(archive_path, root, operands, compression, overwrite:)?
  } else if mode == "list" {
    for entry in archive.tar_list(archive_path, compression, members: operands)? {
      print --flush $entry.path
    }
  } else if mode == "extract" {
    root.mkdir()?

    archive.tar_extract(
      archive_path,
      root,
      strip_components: strip,
      compression:,
      overwrite:,
      members: operands,
    )?
  } else {
    return Err(usage_error("tar", "{c|t|x}f ARCHIVE [FILE...]"))
  }
}
