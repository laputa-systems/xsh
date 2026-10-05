let jobs = 4
let names = ["a", "b"]
# The author's line breaks stay, with one indent and one space before `\`.
run printf "%s %s %s\n"     \
      one "two words"   \
  -j${jobs} ?
# The first part joins the command's name; `?` and `{` join the last line.
run \
  --timeout=5s \
  LC_ALL=C \
  printf "%s\n" options \
  | run tr a-z A-Z \
  2> /dev/null \
  ?
print \
  one \
  two
env FIRST=1 \
  SECOND=2 \
  {
  run printenv SECOND ?
}

proc show(names: List[Str], mode: Int) [process, env, error] {
  if names.len() > 0 {
    # A command that fits stays on one line; one that does not fills lines.
    run printf "%s %s\n" @names (names.len() + 1) ?
    run printf "%s %s %s %s %s %s %s %s %s\n" "-Ddefault_library=shared" "-Ddocumentation=false" "-Ddtd_validation=false" "-Dtests=false" "-Dlibpng=disabled" "-Dgtk=disabled" "-Dtests=disabled" "-Ddemos=disabled" "build" ?
    let listing = run.text printf "%s %s %s %s %s\n" "-Ddefault_library=shared" "-Ddocumentation=false" "-Ddtd_validation=false" "-Dtests=false" "-Dlibpng=disabled" ?
    print $listing "-Ddefault_library=shared" "-Ddocumentation=false" "-Ddtd_validation=false" "-Dtests=false" "-Dlibpng=disabled" "-Dgtk=disabled"
    run printf "%s %s %s\n" "-Ddefault_library=shared" "-Ddocumentation=false" "-Ddtd_validation=false" "-Dlibpng=disabled" | run tr a-z A-Z | run tr A-Z a-z
    # A part too wide for any line stays where it is.
    print f"{names.len()} names: a part that is far too wide to fit on a line of its own is never moved to a continuation line"
    env LC_ALL=C LANG=C LC_COLLATE=C LC_CTYPE=C LC_MESSAGES=C LC_MONETARY=C LC_NUMERIC=C LC_TIME=C LC_ADDRESS=C LC_NAME=C LC_PAPER=C {
      run printenv LC_PAPER ?
    }
  }
  match mode {
    1 => run printf "%s\n" \
      one ?
    _ => print other
  }
  let plan = process.command {
    run printf "%s\n" \
      planned
  }
  let _ = process.run(plan)?
}

show(names, 1)?
