error CoverageError = Failed(message: Str)

pure join_path(entries: List[Path]) -> Str {
  return [entry.display() for entry in entries].join(":")
}

pure repo_path(root: Path, value: Str) -> Path {
  if value.starts_with("/") {
    return fp"${value}"
  }

  return fp"${root}/${value}"
}

proc env_path(root: Path, name: Str, default: Path) [env] -> Path {
  let value = env.get_or(name, "")?.trim()

  if value == "" {
    return default
  }

  return repo_path(root, value)
}

proc cargo_bin_dir(root: Path) [process, env] -> Path {
  let configured = env.get_or("XSH_COV_CARGO_BIN", "")?.trim()

  if configured != "" {
    return repo_path(root, configured)
  }

  match process.which("cargo") {
    Ok(cargo) => return cargo.parent()
    Err(_) => {}
  }

  return /root/.cargo/bin
}

proc rust_host() [process, error] -> Result[Str] {
  for line in run.text rustc -vV?.lines() {
    if line.starts_with("host: ") {
      return line.split(": ")[1]
    }
  }

  return Err(CoverageError.Failed("coverage: rustc -vV did not report a host triple"))
}

proc find_llvm_tool(tool: Str) [fs, process, error] -> Result[Path] {
  let sysroot = fp"${run.text rustc --print sysroot?.trim()}"
  let host = rust_host()?
  let candidates = [fp"${sysroot}/lib/rustlib/${host}/bin/${tool}", fp"${sysroot}/bin/${tool}"]

  for candidate in candidates {
    if candidate.exists()? and fs.executable(candidate)? {
      return candidate
    }
  }

  for entry in fs.walk(sysroot)? {
    if entry.kind == "file" and entry.name == tool and entry.executable {
      return entry.path
    }
  }

  return Err(CoverageError.Failed(f"coverage: could not find ${tool}; install rustup component llvm-tools-preview"))
}

proc remove_dir(target: Path) [fs, error] {
  fs.remove(target, missing_ok: true)?
}

proc collect_profraw(raw_dir: Path) [fs, error] -> Result[List[Str]] {
  var paths = [entry.path.display() for entry in fs.children(raw_dir)?
    |> where .kind == "file" and .name.ends_with(".profraw")
    |> sort-by .path]

  if paths.len() == 0 {
    return Err(CoverageError.Failed(f"coverage: no .profraw files were produced in ${raw_dir.display()}"))
  }

  return paths
}

proc collect_objects(dir: Path) [fs, error] -> Result[List[Str]] {
  var objects: List[Str] = []

  if ! dir.exists()? {
    return objects
  }

  for entry in fs.children(dir)?
    |> where .kind == "file" and .executable
    |> sort-by .path {
    objects = objects.push(entry.path.display())
  }

  return objects
}

pure cov_args(profdata: Path, objects: List[Str]) -> List[Str] {
  var llvm_args = [
    "--instr-profile",
    profdata.display(),
    "--ignore-filename-regex",
    "/(\\.cargo/registry|rustc|target|tests)/",
  ]

  for object in objects {
    llvm_args = llvm_args.push("--object")
    llvm_args = llvm_args.push(object)
  }

  return llvm_args
}

proc main() [fs, process, env, error, io] {
  let root = fs.cwd()?
  let target_dir = env_path(root, "CARGO_TARGET_DIR", fp"${root}/target")
  let out_dir = env_path(root, "XSH_COV_OUT_DIR", fp"${root}/target/cov")
  let raw_dir = fp"${out_dir}/raw"
  let api_dir = fp"${out_dir}/xsh-api"
  let html_dir = fp"${out_dir}/html"
  let shim_dir = fp"${out_dir}/bin"
  let profdata = fp"${out_dir}/xsh.profdata"
  let objects_file = fp"${out_dir}/objects.txt"
  let report_file = fp"${out_dir}/llvm-report.txt"
  let lcov_file = fp"${out_dir}/lcov.info"
  let debug_dir = fp"${target_dir}/debug"
  let deps_dir = fp"${debug_dir}/deps"
  let xsh = fp"${debug_dir}/xsh"
  let xsht = fp"${debug_dir}/xsht"
  let xshi = fp"${debug_dir}/xshi"
  let llvm_profdata = find_llvm_tool("llvm-profdata")?
  let llvm_cov = find_llvm_tool("llvm-cov")?
  let cargo_bin = cargo_bin_dir(root)
  remove_dir(raw_dir)?
  remove_dir(api_dir)?
  remove_dir(html_dir)?
  remove_dir(shim_dir)?
  raw_dir.mkdir()?
  api_dir.mkdir()?
  shim_dir.mkdir()?
  fs.symlink(xsh, fp"${shim_dir}/xsh")?
  fs.symlink(xsht, fp"${shim_dir}/xsht")?
  fs.symlink(xshi, fp"${shim_dir}/xshi")?
  let existing_rustflags = env.get_or("RUSTFLAGS", "")?.trim()

  let rustflags = if existing_rustflags == "" {
    "-C instrument-coverage"
  } else {
    f"${existing_rustflags} -C instrument-coverage"
  }

  let child_path = join_path([shim_dir, cargo_bin, /root/.cargo/bin, /bin, /usr/bin, /usr/local/bin, /sbin])

  env CARGO_TARGET_DIR=$target_dir CARGO_INCREMENTAL=0 LLVM_PROFILE_FILE=fp"${raw_dir}/%m-%p.profraw" PATH=$child_path RUSTFLAGS=$rustflags TZ=UTC XSH_SKIP_LIVE_COREUTILS_COMPARISONS=1 {
    run cargo test -- --test-threads=1 ?
    run cargo test --features linux-priv-tests --test linux_priv -- --test-threads=1 ?
    run cargo build --bin xsh ?
    run cargo build -p xsht ?
    run cargo build -p xshi ?
    run XSHT=$xsht XSH_COV_DIR=$api_dir XSH_COV_JSON=fp"${api_dir}/coverage.json" XSH_COV_REPORT=fp"${api_dir}/coverage.txt" $xsh tools/xsh-cov.xsh ?
  } ?

  let profraws = collect_profraw(raw_dir)?
  run $llvm_profdata merge -sparse -o $profdata @profraws ?
  let objects = collect_objects(debug_dir)?.extend(collect_objects(deps_dir)?) |> sort

  if objects.len() == 0 {
    return Err(CoverageError.Failed(f"coverage: no instrumented objects found under ${debug_dir.display()}"))
  }

  fs.write(
    objects_file,
    f"""${objects.join("\n")}
""",
  )?

  let llvm_args = cov_args(profdata, objects)
  run $llvm_cov report @llvm_args > report_file ?
  io.write_stdout(report_file.read_text()?)?
  let html_output_arg = f"--output-dir=${html_dir.display()}"
  run $llvm_cov show @llvm_args --format=html $html_output_arg --show-instantiations --show-line-counts-or-regions ?
  run $llvm_cov export @llvm_args --format=lcov > lcov_file ?
  print ""
  print "coverage reports:"
  print f"  LLVM summary: ${report_file.strip_prefix(root)?.display()}"
  print f"  LLVM HTML: ${fp"${html_dir}/index.html".strip_prefix(root)?.display()}"
  print f"  LLVM lcov: ${lcov_file.strip_prefix(root)?.display()}"
  print f"  XSH API: ${fp"${api_dir}/coverage.txt".strip_prefix(root)?.display()}"
}
