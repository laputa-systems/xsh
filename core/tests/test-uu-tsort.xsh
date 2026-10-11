##! Transcribed from the uutils coreutils integration tests for tsort.

use support.uu as uu

proc tsort_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tsort", "call_graph.txt", "call_graph.txt")?
  Ok(s)
}

# origin: uutils test_tsort::test_cycle
test test_uu_tsort_cycle { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text("a b b c c d c b"))?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "a\nb\nc\nd\n")
  uu.stderr_is(r, "tsort: -: input contains a loop:\ntsort: b\ntsort: c\n")
}

# origin: uutils test_tsort::test_cycle_loop_from_file
test test_uu_tsort_cycle_loop_from_file { |ctx|
  let s = tsort_scene(ctx)?
  uu.write(s, "f", "t b\nt s\ns t\n")?
  let r = uu.invoke(s, "tsort", ["f"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "s\nt\nb\n")
  uu.stderr_is(r, "tsort: f: input contains a loop:\ntsort: s\ntsort: t\n")
}

# origin: uutils test_tsort::test_cycle_loop_multiple_loops_from_file
test test_uu_tsort_cycle_loop_multiple_loops_from_file { |ctx|
  let s = tsort_scene(ctx)?
  uu.write(s, "f", "a a\na b\na c\nc a\nb a\n")?
  let r = uu.invoke(s, "tsort", ["f"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "a\nc\nb\n")
  uu.stderr_is(r, "tsort: f: input contains a loop:\ntsort: a\ntsort: b\ntsort: f: input contains a loop:\ntsort: a\ntsort: c\n")
}

# origin: uutils test_tsort::test_cycle_loop_with_extra_node_from_file
test test_uu_tsort_cycle_loop_with_extra_node_from_file { |ctx|
  let s = tsort_scene(ctx)?
  uu.write(s, "f", "t x\nt s\ns t\n")?
  let r = uu.invoke(s, "tsort", ["f"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "s\nt\nx\n")
  uu.stderr_is(r, "tsort: f: input contains a loop:\ntsort: s\ntsort: t\n")
}

# origin: uutils test_tsort::test_error_on_dir
test test_uu_tsort_error_on_dir { |ctx|
  let s = tsort_scene(ctx)?
  uu.mkdir(s, "tsort_test_dir")?
  let r = uu.invoke(s, "tsort", ["tsort_test_dir"])?
  uu.fails(r)
  uu.stderr_contains(r, "tsort: tsort_test_dir: read error: Is a directory")
}

# origin: uutils test_tsort::test_help_flag
test test_uu_tsort_help_flag { |ctx|
  let s = tsort_scene(ctx)?
  let short = uu.invoke(s, "tsort", ["-h"])?
  let long = uu.invoke(s, "tsort", ["--help"])?
  uu.fails_with_code(short, 1)
  uu.stderr_only(short, "tsort: invalid option -- 'h'\nTry 'tsort --help' for more information.\n")
  uu.succeeds(long)
  assert short.stdout != long.stdout
  assert long.stdout.utf8()?.starts_with("Usage: tsort")
}

# origin: uutils test_tsort::test_invalid_arg
test test_uu_tsort_invalid_arg { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_tsort::test_invalid_utf8_input
test test_uu_tsort_invalid_utf8_input { |ctx|
  let s = tsort_scene(ctx)?
  uu.write_bytes(s, "a", b"hrllo\nawuyues\napple\niphone\niphone\na\n\xff\n\xff\n")?
  let r = uu.invoke(s, "tsort", ["a"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"apple\nhrllo\n\xff\niphone\nawuyues\na\n")
}

# origin: uutils test_tsort::test_linear_tree_graphs
test test_uu_tsort_linear_tree_graphs { |ctx|
  let s = tsort_scene(ctx)?
  for graph in [{input: "a b b c c d d e e f f g\n", output: "a\nb\nc\nd\ne\nf\ng\n"}, {input: "a b b c c d d e e f f g\nc x x y y z\n", output: "a\nb\nc\nx\nd\ny\ne\nz\nf\ng\n"}, {input: "a b b c c d d e e f f g\nc x x y y z\nf r r s s t\n", output: "a\nb\nc\nx\nd\ny\ne\nz\nf\nr\ng\ns\nt\n"}] {
    let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(graph.input))?
    uu.succeeds(r)
    uu.stdout_only(r, graph.output)
  }
}

# origin: uutils test_tsort::test_long_loop_no_stack_overflow
test test_uu_tsort_long_loop_no_stack_overflow { |ctx|
  let s = tsort_scene(ctx)?
  let count = 100000
  let input = [f"{v} {(v + 1) % count} " for v in range(count)].join("")
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(input))?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "tsort: -: input contains a loop")
}

# origin: uutils test_tsort::test_loop_for_iterative_dfs_correctness
test test_uu_tsort_loop_for_iterative_dfs_correctness { |ctx|
  let s = tsort_scene(ctx)?
  let input = "\n        A B\n        B C\n        C B\n        C D\n        D A\n    "
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(input))?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "tsort: -: input contains a loop:\ntsort: B\ntsort: C")
}

# origin: uutils test_tsort::test_multiple_arguments
test test_uu_tsort_multiple_arguments { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", ["call_graph.txt", "invalid_file"])?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'invalid_file'")
}

# origin: uutils test_tsort::test_no_such_file
test test_uu_tsort_no_such_file { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", ["invalid_file_txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "No such file or directory")
}

# origin: uutils test_tsort::test_nonexistent_file_error_includes_filename
test test_uu_tsort_nonexistent_file_error_includes_filename { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", ["nosuchfile.txt"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tsort: nosuchfile.txt: No such file or directory\n")
}

# origin: uutils test_tsort::test_odd_number_of_tokens
test test_uu_tsort_odd_number_of_tokens { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: b"a\n")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tsort: -: input contains an odd number of tokens\n")
}

# origin: uutils test_tsort::test_only_one_input_file
test test_uu_tsort_only_one_input_file { |ctx|
  let s = tsort_scene(ctx)?
  uu.touch(s, "f")?
  uu.touch(s, "g")?
  let r = uu.invoke(s, "tsort", ["f", "g"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tsort: extra operand 'g'\nTry 'tsort --help' for more information.\n")
}

# origin: uutils test_tsort::test_posix_graph_examples
test test_uu_tsort_posix_graph_examples { |ctx|
  let s = tsort_scene(ctx)?
  for graph in [{input: "a b c c d e\ng g\nf g e f\nh h\n", output: "a\nc\nd\nh\nb\ne\nf\ng\n"}, {input: "b a\nd c\nz h x h r h\n", output: "b\nd\nr\nx\nz\na\nc\nh\n"}] {
    let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text(graph.input))?
    uu.succeeds(r)
    uu.stdout_only(r, graph.output)
  }
}

# origin: uutils test_tsort::test_sort_call_graph
test test_uu_tsort_sort_call_graph { |ctx|
  let s = tsort_scene(ctx)?
  let expected = fp"{ctx.core_dir}/tests/data/uutils/tsort/call_graph.expected".read_bytes()?
  let r = uu.invoke(s, "tsort", ["call_graph.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_tsort::test_sort_floating_nodes
test test_uu_tsort_sort_floating_nodes { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text("d d\nc c\na a\nb b"))?
  uu.succeeds(r)
  uu.stdout_only(r, "a\nb\nc\nd\n")
}

# origin: uutils test_tsort::test_sort_self_loop
test test_uu_tsort_sort_self_loop { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text("first first\nfirst second second second"))?
  uu.succeeds(r)
  uu.stdout_only(r, "first\nsecond\n")
}

# origin: uutils test_tsort::test_split_on_any_whitespace
test test_uu_tsort_split_on_any_whitespace { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r)
  uu.stdout_only(r, "a\nb\n")
}

# origin: uutils test_tsort::test_tsort_non_utf8_paths
test test_uu_tsort_tsort_non_utf8_paths { |ctx|
  let s = tsort_scene(ctx)?
  let name = Path.parse_bytes(b"\xff\xfe")?
  uu.at_bytes(s, b"\xff\xfe")?.write(b"a b\nb c\n")?
  let r = uu.invoke_paths(s, "tsort", [name])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\nb\nc\n")
}

# origin: uutils test_tsort::test_two_cycles
test test_uu_tsort_two_cycles { |ctx|
  let s = tsort_scene(ctx)?
  let r = uu.invoke(s, "tsort", [], stdin: bytes.from_text("a b b c c b b d d b"))?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "a\nb\nd\nc\n")
  uu.stderr_is(r, "tsort: -: input contains a loop:\ntsort: b\ntsort: c\ntsort: -: input contains a loop:\ntsort: b\ntsort: d\n")
}

# origin: uutils test_tsort::test_version_flag
test test_uu_tsort_version_flag { |ctx|
  let s = tsort_scene(ctx)?
  let short = uu.invoke(s, "tsort", ["-V"])?
  let long = uu.invoke(s, "tsort", ["--version"])?
  uu.fails_with_code(short, 1)
  uu.stderr_only(short, "tsort: invalid option -- 'V'\nTry 'tsort --help' for more information.\n")
  uu.succeeds(long)
  assert short.stdout != long.stdout
  assert long.stdout.utf8()?.starts_with("tsort (")
}

# origin: uutils test_tsort::test_write_error
test test_uu_tsort_write_error { |ctx|
  let s = tsort_scene(ctx)?
  uu.write(s, "input", "a d\n")?
  let r = uu.invoke(s, "tsort", ["input"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_contains(r, "write error")
  uu.stderr_contains(r, "No space left on device")
}

