use support.uu
use support.util_linux as ul

proc parity(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "lslocks", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lslocks", args, vars: vars, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

test test_lslocks_list_columns_modes { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-H"], ["--list-columns", "--raw"], ["-H", "--json"]] {
    parity(s, args)?
  }
}

test test_lslocks_raw_noheadings_and_json { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [["--raw"], ["--noheadings"], ["--json"], ["--noheadings", "--json"]] {
    parity(s, args)?
  }
  parity(s, ["--json", "--raw"])?
}

test test_lslocks_defaults_and_lslocks_columns { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  parity(s, [])?
  parity(s, ["--output-all"])?
  parity(s, ["--output-all"], {LSLOCKS_COLUMNS: "TYPE"})?
  parity(s, [], {LSLOCKS_COLUMNS: "TYPE,PID,COMMAND"})?
  parity(s, [], {LSLOCKS_COLUMNS: "+BLOCKER"})?
  parity(s, ["--output", "PATH,TYPE"], {LSLOCKS_COLUMNS: "PID"})?
}

test test_lslocks_output_column_names_and_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [
    ["--output", "path,type"], ["--output", "+BLOCKER"],
    ["--output", "PID,PID"], ["--output", ""], ["--output", "UNKNOWN"],
    ["--output", "PID", "--output", "TYPE"],
  ] {
    parity(s, args)?
  }
}

test test_lslocks_help_and_usage_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [
    ["-h"], ["--help"], ["--output"], ["--pid", "bad"], ["--unknown"],
    ["--json", "--raw"], ["operand"],
  ] {
    parity(s, args)?
  }
}

test test_lslocks_smartcols_json_environment { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for value in ["compact", "lines", "unknown"] {
    parity(s, ["--json"], {LIBSMARTCOLS_JSON: value})?
  }
  parity(s, ["--json"])?
}

test test_lslocks_filter_errors { |ctx|
  let s = uu.scene(ctx)?
  for expression in ["TYPE ==", "UNKNOWN == \"POSIX\""] {
    parity(s, ["--filter", expression])?
  }
}

test test_lslocks_version_provenance { |ctx|
  let s = uu.scene(ctx)?
  let reference = ul.reference(s, "lslocks", ["--version"], timeout: 5s)?
  let candidate = uu.invoke(s, "lslocks", ["--version"], timeout: 5s)?
  ul.version_identity(reference, candidate)
}
