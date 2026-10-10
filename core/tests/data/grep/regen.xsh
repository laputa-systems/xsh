#!/usr/bin/env -S xsh --
# Rebuild expected.txt from GNU grep in the throwaway oracle container.
#
#   xsh core/tests/data/grep/regen.xsh
#
# Needs docker and dev/compat/oracle.sh. Every case in cases.jsonl runs inside
# the container from a copy of the fx/ fixture tree; its exit status, stdout
# and stderr are written to expected.txt in the line format that
# core/tests/test-grep.xsh compares against. Review the diff before
# committing: an unexpected change means GNU grep behaved differently from the
# release the cases were captured with.

pure shell_quote(text: Str) -> Str {
  let inner = text.replace("'", with: "'\\''")
  f"'{inner}'"
}

pure escape(data: Bytes) -> Str {
  let digits = "0123456789abcdef"
  var pieces: List[Str] = []
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte == 92 { pieces += ["\\\\"] } else if byte == 10 { pieces += ["\\n"] } else if byte == 9 { pieces += ["\\t"] } else if byte == 13 { pieces += ["\\r"] } else if byte == 0 { pieces += ["\\0"] } else if byte >= 32 and byte < 127 { pieces += [data.slice(index, length: 1).utf8() ?? "?"] } else {
      pieces += [f"\\x{digits.byte_slice(byte / 16, 1)}{digits.byte_slice(byte % 16, 1)}"]
    }
  }
  pieces.join("")
}

proc main() [fs, process, io, error] {
  let here = (process.script_path() ?? p"regen.xsh").parent()
  let root = here.parent().parent().parent().parent()
  let scratch = fs.tempdir()?
  defer scratch.close()
  let work = scratch.host_path()?
  let _ = fs.copy_tree(fp"{here}/fx", fp"{work}/fx")?
  fp"{work}/out".mkdir()

  var driver = "#!/bin/sh\ncd /fixture/fx\n"
  var ids: List[Str] = []
  for line in fp"{here}/cases.jsonl".read_lines()? {
    let doc = json.decode(line)?
    let id = json.get(doc, ["id"])?.require(Str)?
    let cmd = json.get(doc, ["cmd"])?.require(Str)?
    let args = json.get(doc, ["args"])?.require(List[Str])?
    let stdin = json.get(doc, ["stdin"])?.require(Str)?
    let lc = json.get(doc, ["lc"])?.require(Str)?
    let colors = json.get(doc, ["colors"])?.require(Str)?
    let color = json.get(doc, ["color"])?.require(Str)?
    let posixly = json.get(doc, ["posixly"])?.require(Bool)?
    let quoted = [shell_quote(arg) for arg in args].join(" ")
    let input = if stdin == "" { "/dev/null" } else { shell_quote(stdin) }
    let extra = if posixly { " POSIXLY_CORRECT=1" } else { "" }
    driver += f"timeout 30 env -i PATH=/usr/bin:/bin LC_ALL={shell_quote(lc)} GREP_COLORS={shell_quote(colors)} GREP_COLOR={shell_quote(color)}{extra} {cmd} {quoted} < {input} > /fixture/out/{id}.out 2> /fixture/out/{id}.err; echo $? > /fixture/out/{id}.rc\n"
    ids += [id]
  }
  fp"{work}/driver.sh".write(driver)
  fp"{work}/driver.sh".chmod(0o755)

  let oracle = fp"{root}/dev/compat/oracle.sh"
  let plan = process.command_argv(oracle, ["oracle.sh", "--mount", work.display(), "--rw", "--", "/fixture/driver.sh"])
  let finished = process.run(plan)?
  assert finished.exit_code()? == 0, "the oracle driver failed"

  var lines: List[Str] = []
  for id in ids {
    let status = fp"{work}/out/{id}.rc".read_text()?.trim()
    lines += [f"@ {id}", f"rc {status}", f"out {escape(fp"{work}/out/{id}.out".read_bytes()?)}", f"err {escape(fp"{work}/out/{id}.err".read_bytes()?)}"]
  }
  fp"{here}/expected.txt".write_lines(lines)
  print f"wrote {ids.len()} cases to {here}/expected.txt"
}
