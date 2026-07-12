# Profile-guided optimization (PGO) build for the `xsh` binary.
#
# Three phases, all on the `profiling` profile with the `net` feature excluded
# (tools-only), driven by the same workload `make prof` profiles elsewhere:
#   1. generate: build an instrumented xsh, then exercise it via the perf
#      scenarios and the analysis showcases run over this repo -> .profraw
#      frequency counters.
#   2. merge:    llvm-profdata merge (NOT -sparse; sparse is for coverage).
#   3. use:      rebuild xsh with -Cprofile-use -> the PGO-optimized binary.
#
# Build mechanics mirror tools/cov-linux.xsh exactly (native musl host build, no
# --target / -Z build-std, RUSTFLAGS via env) so this works in Dockerfile.test
# under the same configuration coverage already relies on. Run via `make prof`.
error ProfileError = Failed(message: Str)

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
  let configured = env.get_or("XSH_PROF_CARGO_BIN", "")?.trim()

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
  let version: Str = run.text rustc -vV ?

  for line in version.lines() {
    if line.starts_with("host: ") {
      return line.split(": ")[1]
    }
  }

  return Err(ProfileError.Failed("prof: rustc -vV did not report a host triple"))
}

proc find_llvm_tool(tool: Str) [fs, process, error] -> Result[Path] {
  let sysroot_raw: Str = run.text rustc --print sysroot ?
  let sysroot = fp"${sysroot_raw.trim()}"
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

  return Err(ProfileError.Failed(f"prof: could not find ${tool}; install rustup component llvm-tools-preview"))
}

proc remove_dir(target: Path) [fs, error] {
  fs.remove(target, missing_ok: true)?
}

proc collect_profraw(raw_dir: Path) [process, error] -> Result[List[Str]] {
  let find_output: Str = run.text find $raw_dir -type f -name "*.profraw" ?

  var paths = [
    line
    for line in find_output.lines()
      |> where .trim() != ""
      |> sort
  ]

  if paths.len() == 0 {
    return Err(ProfileError.Failed(f"prof: no .profraw files were produced in ${raw_dir.display()}"))
  }

  return paths
}

proc xsh_files(dir: Path) [fs, error] -> Result[List[Path]] {
  return [
    entry.path
    for entry in fs.children(dir)?
      |> where .kind == "file" and .path.ext() == "xsh" and ! .name.ends_with("_helper.xsh")
      |> sort-by .name
  ]
}

proc main() [fs, process, env, error, io] {
  let root = fs.cwd()?
  let target_dir = env_path(root, "CARGO_TARGET_DIR", fp"${root}/target")
  let pgo_dir = env_path(root, "XSH_PGO_OUT_DIR", fp"${root}/target/pgo")
  let gen_target = fp"${target_dir}/pgo-gen"
  let use_target = fp"${target_dir}/pgo-use"
  let raw_dir = fp"${pgo_dir}/raw"

  # The merged profile is checked in per host triple. Release builds currently
  # consume it only when RELEASE_USE_PGO=1; see perf/pgo/README.md and PGO.md.
  let profdata_dir = fp"${root}/perf/pgo"
  let host = rust_host()?
  let default_profdata = fp"${profdata_dir}/xsh-${host}.profdata"
  let profdata = env_path(root, "XSH_PGO_PROFILE", default_profdata)
  let corpus = fp"${pgo_dir}/corpus"
  let gen_xsh = fp"${gen_target}/profiling/xsh"
  let use_xsh = fp"${use_target}/profiling/xsh"
  let scale = env.int("XSH_PROF_SCALE", 8)?
  let scenarios = xsh_files(fp"${root}/perf/scenarios")?
  let interpreter_scripts = xsh_files(fp"${root}/perf/interpreter")?
  let startup_scripts = xsh_files(fp"${root}/perf/startup")?
  let frontend_scripts = xsh_files(fp"${root}/perf/frontend")?
  let hot_scenario_repeat = env.int("XSH_PGO_HOT_SCENARIO_REPEAT", 24)?
  let archive_repeat = env.int("XSH_PGO_ARCHIVE_REPEAT", 4)?
  let interpreter_repeat = env.int("XSH_PGO_INTERPRETER_REPEAT", 8)?
  let startup_repeat = env.int("XSH_PGO_STARTUP_REPEAT", 120)?
  let frontend_repeat = env.int("XSH_PGO_FRONTEND_REPEAT", 32)?
  let llvm_profdata = find_llvm_tool("llvm-profdata")?
  let cargo_bin = cargo_bin_dir(root)
  let child_path = join_path([cargo_bin, /root/.cargo/bin, /bin, /usr/bin, /usr/local/bin])
  remove_dir(raw_dir)?
  pgo_dir.mkdir()?
  raw_dir.mkdir()?
  profdata_dir.mkdir()?
  remove_dir(corpus)?

  # Gen and use RUSTFLAGS must be identical except generate<->use, or function
  # hashes mismatch and PGO yields no benefit.
  let existing_rustflags = env.get_or("RUSTFLAGS", "")?.trim()
  let base = existing_rustflags

  let gen_flags = if base == "" {
    f"-Cprofile-generate=${raw_dir.display()}"
  } else {
    f"${base} -Cprofile-generate=${raw_dir.display()}"
  }

  let use_flags = if base == "" {
    f"-Cprofile-use=${profdata.display()} -Cllvm-args=-pgo-warn-missing-function"
  } else {
    f"${base} -Cprofile-use=${profdata.display()} -Cllvm-args=-pgo-warn-missing-function"
  }

  let scale_arg = f"${scale}"

  # Phase 1: build + run the instrumented workload (profile-generate). %m-%p in
  # LLVM_PROFILE_FILE is mandatory: the workload spawns many distinct binaries
  # (test bins, the bench bin, xsh) that would otherwise overwrite each other.
  env CARGO_TARGET_DIR=$gen_target CARGO_INCREMENTAL=0 LLVM_PROFILE_FILE=fp"${raw_dir}/%m-%p.profraw" PATH=$child_path RUSTFLAGS=$gen_flags TZ=UTC XSH_SKIP_LIVE_COREUTILS_COMPARISONS=1 {
    run cargo build --profile profiling --no-default-features --features tools --bin xsh ?

    # The PGO workload is representative xsh-binary execution only: the perf
    # scenarios and the real-codebase showcases below. `cargo test` is omitted (its
    # subprocess tests need the cov-style multi-binary shim PATH, which conflicts
    # with this xsh-only build) and `cargo bench` is omitted (the bench binary
    # links dev-dependencies like criterion/syn, which bloat the checked-in profile
    # with counters the xsh release never uses). The showcases over this repo
    # exercise the interpreter's parse/check/eval hot paths heavily on their own.
    run $gen_xsh perf/make-corpus.xsh -- --root $corpus --scale $scale_arg ?

    for scenario in scenarios {
      let repeat_count = if scenario.name == "archive-package.xsh" { archive_repeat } else { hot_scenario_repeat }
      var repeat = 0

      while repeat < repeat_count {
        run $gen_xsh $scenario -- $corpus > /dev/null ?
        repeat += 1
      }
    }

    for script in interpreter_scripts {
      var repeat = 0

      while repeat < interpreter_repeat {
        run $gen_xsh $script > /dev/null ?
        repeat += 1
      }
    }

    for script in startup_scripts {
      var repeat = 0

      while repeat < startup_repeat {
        run $gen_xsh $script > /dev/null ?
        repeat += 1
      }
    }

    for script in frontend_scripts {
      var repeat = 0

      while repeat < frontend_repeat {
        run $gen_xsh $script > /dev/null ?
        repeat += 1
      }
    }

    # Real-codebase execution: run the analysis showcases over this repo's own
    # source (Rust + xsh + docs) so PGO sees heavy, representative interpreter
    # work — fs walks, multi-language parsing, regex, records, and pipelines —
    # beyond the synthetic scenarios. Best-effort: a showcase exiting non-zero
    # must not abort the build, so capture status via run.status and ignore it.
    for dir in ["src", "core", "showcase", "tests", "docs"] {
      run.status $gen_xsh showcase/tokei.xsh -- $dir > /dev/null 2> /dev/null
      run.status $gen_xsh showcase/loc.xsh -- $dir > /dev/null 2> /dev/null
      run.status $gen_xsh showcase/ecount.xsh -- $dir > /dev/null 2> /dev/null
    }

    run.status $gen_xsh showcase/tokei.xsh -- --json src > /dev/null 2> /dev/null
    run.status $gen_xsh showcase/file-report.xsh -- --root src > /dev/null 2> /dev/null
    run.status $gen_xsh showcase/secret-scan.xsh -- --root src > /dev/null 2> /dev/null
    run.status $gen_xsh showcase/todo-scan.xsh -- --root src > /dev/null 2> /dev/null
    run.status $gen_xsh showcase/dedup.xsh -- --root src > /dev/null 2> /dev/null
    run.status $gen_xsh showcase/rgrep.xsh -- --pattern fn --root src > /dev/null 2> /dev/null
  } ?

  # Phase 2: merge (plain merge, not -sparse).
  let profraws = collect_profraw(raw_dir)?
  run $llvm_profdata merge -o $profdata @profraws ?

  # Phase 3: rebuild optimized with the merged profile.
  env CARGO_TARGET_DIR=$use_target CARGO_INCREMENTAL=0 PATH=$child_path RUSTFLAGS=$use_flags TZ=UTC {
    run cargo build --profile profiling --no-default-features --features tools --bin xsh ?
  } ?

  remove_dir(corpus)?

  # Copy the optimized binary into pgo_dir (bind-mounted out of the container).
  let out_xsh = fp"${pgo_dir}/xsh"
  run cp $use_xsh $out_xsh ?
  print ""
  print "PGO build complete:"
  print f"  profile data: ${profdata.strip_prefix(root)?.display()}  (checked in; release PGO is opt-in)"
  print f"  pgo binary:   ${use_xsh.strip_prefix(root)?.display()}"
  print f"  copied to:    ${out_xsh.strip_prefix(root)?.display()}"
}
