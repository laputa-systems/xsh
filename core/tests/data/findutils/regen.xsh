##! Regenerates the GNU findutils corpora next to this file.
##!
##! Run from the repository root with a container runtime available:
##!   xsh core/tests/data/findutils/regen.xsh -- find|xargs
##! It builds the fixture tree, runs every case under the reference
##! implementation through dev/compat/oracle.sh, and rewrites the JSON that the
##! applet tests compare against. The container is only an author-time oracle;
##! the tests need none.
use core.lib.findutils_fixture as fixture
use core.tests.data.findutils.find_cases as find_cases
use core.tests.data.findutils.xargs_cases as xargs_cases

pure quote(word: Str) -> Str {
  f"'{word.replace("'", with: "'\\''")}'"
}

# The oracle container is musl-based, whose strerror for ELOOP differs from
# glibc's; the applets print the glibc wording that GNU systems show.
pure glibc_wording(data: Bytes) -> Bytes {
  match data.utf8() {
    Ok(text) => bytes.from_text(text.replace("Symbolic link loop", with: "Too many levels of symbolic links"))
    Err(_) => data
  }
}

# The oracle's xargs aborts with SIGSEGV (status 139) in a few paths that GNU
# xargs documents: a child exiting 255 or dying from a signal ends the run with
# status 124 or 125. Those cases record the documented status instead; any
# other crash fails the regeneration.
pure documented_status(name: Str) -> Int {
  match name {
    "exit_255" | "exit_255_stops" | "exit_status_precedence" | "P_255_stops" | "exit_zero_255" => 124
    "exit_signal" | "exit_signal_term" | "cmd_status_125_sig_after_fail" => 125
    _ => -1
  }
}

# `run NAME FLAGS COMMAND...` leaves out/NAME.{out,err,status}. A `d` case first
# gets a fresh small tree named scratch and afterwards appends the
# `cap` file it wrote and a bytewise-sorted listing of scratch, which is what
# the tests' harness reconstructs on the applet side.
pure driver_function() -> Str {
  r"""
  run() {
    name=$1; flags=$2; shift 2
    case $flags in *d*)
      rm -rf scratch; mkdir scratch
      cp -a tree/sub tree/a.txt tree/ro.txt tree/hard1 tree/hard2 tree/link_a tree/broken tree/empty_dir scratch/ ;;
    esac
    case $flags in *r*)
      rm -rf tree/recent; mkdir tree/recent; now=$(date +%s)
      for entry in m5:330 h2:9030 d3:302400 d10:907200; do
        echo r > tree/recent/${entry%:*}; touch -d @$((now - ${entry#*:})) tree/recent/${entry%:*}
      done ;;
    esac
    in=/dev/null; [ -f in/$name ] && in=in/$name
    ( case $flags in *t*) cd tree ;; esac; "$@" ) <$in >out/$name.out 2>out/$name.err
    echo $? >out/$name.status
    case $flags in *d*)
      { echo '--- after ---'; [ -f scratch/cap ] && { echo cap:; cat scratch/cap; }
        find scratch -print0 | sort -z | tr '\0' '\n'; } >>out/$name.out ;;
    esac
  }
  """
}

proc main(program: Str) {
  let cases = if program == "find" { find_cases.all() } else { xargs_cases.all() }
  let identity = unix.id()?
  tempdir work {
    let root = fp"{work}"
    fixture.build(root)?
    fixture.build_hostile(root)?
    fp"{root}/out".mkdir()?
    fp"{root}/in".mkdir()?
    var script: List[Str] = ["cd /fixture", "export TZ=UTC LC_ALL=C", driver_function()]
    for c in cases {
      if ! c.input.is_empty() { fp"{root}/in/{c.name}".write(fixture.expand_input(c.input))? }
      var words: List[Str] = []
      for arg in c.args { words += [quote(fixture.resolve_ids(arg, identity.uid, identity.gid))] }
      script += [f"run {c.name} '{c.flags}' {program} {words.join(" ")}"]
    }
    fp"{root}/driver.sh".write(script.join("\n") + "\n")?
    let oracle = fp"{fs.cwd()?}/dev/compat/oracle.sh"
    let status = run.status $oracle --mount $root --rw -- sh /fixture/driver.sh
    assert status.exited_with(0)
    fixture.restore(root)?
    var records: List[fixture.Stored] = []
    for c in cases {
      var stdout = fp"{root}/out/{c.name}.out".read_bytes()?
      var stderr = fp"{root}/out/{c.name}.err".read_bytes()?
      var code = (fp"{root}/out/{c.name}.status".read_text()?).trim().parse_int() ?? -1
      stderr = glibc_wording(stderr)
      if code == 139 {
        let documented = documented_status(c.name)
        assert documented > 0, f"{c.name}: the oracle crashed"
        code = documented
      }
      if "u" in c.flags { stdout = fixture.mark_ids(stdout, identity.uid, identity.gid) }
      if "l" in c.flags { stdout = fixture.normalize_ls(stdout) }
      records += [{name: c.name, flags: c.flags, args: c.args, input: fixture.stored_text(c.input), input_hex: fixture.stored_hex(c.input), status: code, stdout: fixture.stored_text(stdout), stdout_hex: fixture.stored_hex(stdout), stderr: fixture.stored_text(stderr), stderr_hex: fixture.stored_hex(stderr)}]
    }
    json.write_lines(fp"{fs.cwd()?}/core/tests/data/findutils/{program}.jsonl", records)?
  }
}
