use core.lib.findutils_fixture as fixture

test test_find_corpus_checks_live_directory_size { |ctx|
  let root = test.temp_dir(ctx, name: "find-directory-size")?
  let work = test.temp_dir(ctx, name: "find-directory-size-output")?
  fixture.build(root)

  let corpus = fp"{root}/one.jsonl"
  let stored = fixture.Stored(
    name: "printf_s",
    flags: "s",
    args: ["tree/sizes", "-printf", "%s %f\\n"],
    input: "",
    input_hex: "",
    status: 0,
    stdout: "180 sizes\n2049 n2049\n1025 n1025\n1024 n1024\n513 n513\n512 n512\n511 n511\n1 n1\n",
    stdout_hex: "",
    stderr: "",
    stderr_hex: "",
  )
  corpus.write(f"{json.encode(stored)?}\n")

  let script = fp"{ctx.core_dir}/find.xsh"
  let matched = fixture.run_corpus(ctx.xsh_bin, script, corpus, root, work, 0, 1)?
  assert matched.is_empty(), matched.join("\n")

  let wrong_xsh = fp"{root}/wrong-xsh"
  wrong_xsh.write(f"#!/bin/sh\n\"{ctx.xsh_bin}\" \"$@\" | sed 's/^[0-9][0-9]* sizes$/0 sizes/'\n", mode: 0o755)
  wrong_xsh.chmod(0o755)
  let wrong_size = fixture.run_corpus(wrong_xsh, script, corpus, root, work, 0, 1)?
  assert wrong_size.len() == 1, wrong_size.join("\n")
  assert wrong_size[0].starts_with("printf_s: stdout"), wrong_size[0]
}
