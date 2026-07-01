let scratch_handle = fs.tempdir()?
defer fs.close_root(scratch_handle)?
let scratch = fs.root_path(scratch_handle)?
let mode = 0o755
let label = f"mode ${mode}"
let raw_lines = run.stream --text printf "%s\n" alpha beta gamma

let lines = raw_lines
  |> drop(1)
  |> take(1)

let shuffled = [1, 2, 3] |> shuffle(7)

error FsError = NotFound(message: Str) : NotFound

let _ = process.command {
  timeout = 2s
  run --timeout=1s echo ok
}

match Err(FsError.NotFound(message: "missing")) {
  Err(FsError.NotFound {message: _}) => print ${mode == 493} ${"493" in label} lines[0] ${shuffled |> count()}
}
