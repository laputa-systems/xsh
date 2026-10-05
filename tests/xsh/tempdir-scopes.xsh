proc tempdir_early_return() [fs, error] -> Result[Path] {
  tempdir dir {
    fp"{dir}/kept".write("x")
    return Ok(dir)
  }
  Ok(/)
}

test test_tempdir_binds_a_fresh_directory_removed_after_the_body {
  var seen = /
  var cleanup = ""
  tempdir dir {
    defer {
      cleanup = f"cleanup sees {fs.children(dir)? |> count()}"
    }
    assert (fs.children(dir)? |> count()) == 0
    fp"{dir}/notes.txt".write("hi\n")
    seen = dir
  }
  assert cleanup == "cleanup sees 1"
  assert ! seen.exists()?
}

test test_tempdir_value_scope_returns_the_tail_as_a_result {
  let count = tempdir work {
    fp"{work}/x".write("1")
    fp"{work}/y".write("2")
    fs.files(work)? |> count()
  }
  assert count == Ok(2)
  let name = tempdir work { work.name() }?
  assert ! name.is_empty()
}

test test_tempdir_removes_on_return_loop_control_and_failure {
  let returned = tempdir_early_return()?
  assert ! returned.exists()?
  var visited = []
  for index in range(3) {
    tempdir dir {
      visited += [dir]
      continue when index == 1
      break when index == 2
    }
  }

  assert visited.len() == 3
  for dir in visited {
    assert ! dir.exists()?
  }

  var failed_dir = /
  let failed = try {
    tempdir dir {
      failed_dir = dir
      fp"{dir}/missing/file".write("x")?
    }
  }
  assert failed is Err(_)
  assert failed_dir != /
  assert ! failed_dir.exists()?
}

test test_tempdir_producer_keeps_its_directory_between_pulls { |ctx|
  let output = test.expect(
    ctx,
    r"""
var made: List[Path] = []
stream names() [fs, error] -> Stream[Str] {
  tempdir dir {
    made += [dir]
    fp"{dir}/a".write("1")?
    fp"{dir}/b".write("2")?
    for file in fs.files(dir)? |> sort-by .path {
      yield f"{file.path.name()} {dir.exists()?}"
    }
  }
}
print ${(names() |> collect()).join(",")}
print ${names() |> take(1) |> first()?}
print ${made |> where { .exists()? } |> count()}
""",
    status: 0,
  )?
  assert output.stdout == "a true,b true\na true\n0\n"
}

test test_tempdir_is_contextual_and_checked { |ctx|
  let tempdir = "still a name"
  assert tempdir.byte_len() == 12
  for {source, code} in [
    {source: "pure p() -> Int {\n  tempdir d { let _ = d }\n  1\n}\n", code: "check.pure-effect"},
    {source: "proc p() [error] {\n  tempdir d { let _ = d }\n  print done\n}\n", code: "check.effect-violation"},
    {source: "proc p() [fs] {\n  tempdir d { let _ = d }\n  print done\n}\n", code: "check.effect-violation"},
    {source: "tempdir d { |x| print $x }\n", code: "parse.block-params"},
    {source: "tempdir d { d = p\"/\" }\n", code: "check.assign-let"},
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert code in output.stderr, f"{source}: {output.stderr}"
  }
}

test test_tempdir_and_item_blocks_format_stably { |ctx|
  let source = r"""
    tempdir   stage {
    fp"{stage}/a".write("1")?
    let sizes = fs.files(stage)? |> map {   .size }
    print ${sizes |> count()}
    }
    let count = tempdir work { fs.files(work)? |> count() }?
    print $count
    """
  let candidate = test.temp_file(ctx, name: "tempdir-format.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  # A braced item block and the unbraced stage argument are one form.
  assert candidate.read_text()?.trim() == r"""
    tempdir stage {
      fp"{stage}/a".write("1")?
      let sizes = fs.files(stage)? |> map .size
      print ${sizes |> count()}
    }
    let count = tempdir work { fs.files(work)? |> count() }?
    print $count
    """
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == "1\n0\n"
}
