# syntax fixture
use fs
use path

type Source = {path: Path, kind: Str}

type Sources = List[Source]

type SourceMap = Map[Source]

error CompileError = Failed(message: Str)

pure object_path(src: Str) -> Path {
  return fp"${src}.o"
}

proc compile(src: Path, obj: Path, mode: Str = "opt", ...labels: List[Str]) {
  let flags: List[Str] = ["-O2", "-DNDEBUG"]
  var status = run.status cc @flags -c ${src} -o $obj ?

  while false {
    continue
  }

  match status {
    s if s.ok => print "compiled "${src},
    s if s.exited_with(1) => eprint ${mode},
    _ => return Err(CompileError.Failed(message: "failed"))
  }

  for flag in flags {
    print ${flag}
  }

  for label in labels {
    print ${label}
  }
}

compile(p"main.c", p"main.o")?
