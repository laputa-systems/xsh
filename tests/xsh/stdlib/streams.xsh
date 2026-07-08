proc test_stream_adapters_and_transform_stages() [fs, process, error] {
  let lines = """alpha
beta
""" |> text.lines

  test.eq(lines, ["alpha", "beta"])?
  let chunks = b"abcdef" |> bytes.chunks(2)
  test.ok(chunks[0] == b"ab")?
  test.ok(chunks[2] == b"ef")?

  let json_lines = """{"name":"alpha","size":2}
{"name":"beta","size":1}
"""
  |> json.lines
  |> sort-by .size

  test.eq(json_lines[0].name, "beta")?

  let json_stream = """{"ok":true}
{"ok":false}
""" |> json.stream

  test.eq(json_stream[1].ok, false)?

  test.eq(
    [3, 1, 2, 2]
      |> where . > 1
      |> sort
      |> unique-by .
      |> map { |n|
        n * 2
      },
    [4, 6],
  )?

  test.eq([1, 2, 3, 4] |> take(2), [1, 2])?
  test.eq([1, 2, 3, 4] |> drop(2), [3, 4])?
  test.eq([1, 2] |> repeat(2), [1, 2, 1, 2])?
  test.eq([0] |> range(1, 4), [1, 2, 3])?
  test.eq([0] |> range(4, 1), [4, 3, 2])?

  test.eq(
    ["ab", "c"]
      |> flat-map { |word|
        word.split("")
      },
    ["a", "b", "c"],
  )?

  test.eq(
    [1, 2, 3]
      |> fold(0) { |acc|
        acc + .
      },
    6,
  )?

  test.eq(
    [1, 2, 3]
      |> reduce(10) { |acc|
        acc + .
      },
    16,
  )?

  test.eq([1, 2, 3] |> sum, 6)?
  test.eq(([3, 1, 2] |> min)?, 1)?
  test.eq(([3, 1, 2] |> max)?, 3)?
  test.eq(([3, 1, 2] |> first())?, 3)?
  test.eq(([3, 1, 2] |> last())?, 2)?
  test.ok([1, 2, 3] |> any . == 2)?
  test.ok([1, 2, 3] |> all . > 0)?
  let expected_counts: Map[Int] = map.empty().set("1", 2).set("2", 1)

  test.eq(
    ["a", "bb", "c"]
      |> count { |word|
        word.count_chars()
      },
    expected_counts,
  )?

  test.eq(
    [1, 2, 3]
      |> par-map { |value|
        value * 2
      },
    [2, 4, 6],
  )?

  test.eq([1, 2, 3, 4] |> batch --count=2, [[1, 2], [3, 4]])?
  let enumerated = ["x", "y"] |> enumerate()
  test.eq(enumerated[1].index, 1)?
  test.eq(enumerated[1].value, "y")?
  let zipped = ["left", "right"] |> zip([10, 20])
  test.eq(zipped[0].left, "left")?
  test.eq(zipped[1].right, 20)?

  let groups = [{kind: "a", value: 1}, {kind: "b", value: 2}, {kind: "a", value: 3}]
    |> group-by .kind
    |> sort-by .key

  test.eq(groups[0].key, "a")?
  test.eq(groups[0].items.len(), 2)?
  let shuffled = [1, 2, 3, 4] |> shuffle(7)
  test.eq(shuffled.len(), 4)?
  test.eq(shuffled |> sort, [1, 2, 3, 4])?

  [1, 2]
    |> each { |value|
      test.ok(value > 0)?
    }

  test.eq(
    [1, 2]
      |> tee { |value|
        test.ok(value > 0)?
      }
      |> map { |value|
        value + 1
      },
    [2, 3],
  )?

  [{name: "small", size: 1}, {name: "large", size: 4}] |> table.print(columns: ["name", "size"])
}

proc test_core_commands_and_byte_pipeline(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "core-byte-pipeline")?
  let output = fp"${root}/out.txt"

  cd root {
    fs.write(p"inside.txt", "cwd")?
  }

  test.eq(fp"${root}/inside.txt".read_text()?, "cwd")?
  eprint "covered stderr"
  run printf "%s" "abc" | run tr a-z A-Z > output ?
  test.eq(output.read_text()?, "ABC")?
}

proc test_reduce_by_stream_aggregates() [error] {
  # `reduce-by` keeps one accumulator per key without group-by materialization.
  let nums = [1, 2, 3, 4, 5, 6]
  let agg = nums |> reduce-by --sum { |n|
    {key: if n % 2 == 0 { "even" } else { "odd" }, value: {count: 1, total: n}}
  }
  let lo = nums |> reduce-by --min { |n|
    {key: "all", value: n}
  }
  let hi = nums |> reduce-by --max { |n|
    {key: "all", value: n}
  }

  test.eq(agg.get("even", {count: 0, total: 0}), {count: 3, total: 12})?
  test.eq(agg.get("odd", {count: 0, total: 0}), {count: 3, total: 9})?
  test.eq(lo.get("all", 0), 1)?
  test.eq(hi.get("all", 0), 6)?
}

proc test_reduce_by_parallel_jobs_match_serial() [error] {
  # `--jobs=N` folds partitions on worker threads and merges associative partials.
  let nums = [0] |> range(0, 50000)
  let serial = nums |> reduce-by --sum { |n|
    {key: if n % 3 == 0 { "a" } else if n % 3 == 1 { "b" } else { "c" }, value: {count: 1, total: n}}
  }
  let par = nums |> reduce-by --sum --jobs=8 { |n|
    {key: if n % 3 == 0 { "a" } else if n % 3 == 1 { "b" } else { "c" }, value: {count: 1, total: n}}
  }

  for k in serial.keys() {
    test.eq(par.get(k, {count: 0, total: 0}), serial.get(k, {count: 0, total: 0}))?
  }

  test.eq(par.keys().len(), 3)?
  test.eq((nums |> reduce-by --min --jobs=8 { |n| {key: "all", value: n} }).get("all", -1), 0)?
  test.eq((nums |> reduce-by --max --jobs=8 { |n| {key: "all", value: n} }).get("all", -1), 49999)?
}

proc test_par_map_reduce_by_fuses_to_worker_aggregation() [error] {
  let nums = [0] |> range(0, 50000)
  let fused = nums
    |> par-map --jobs=8 { |n|
      {bucket: if n % 4 == 0 { "a" } else if n % 4 == 1 { "b" } else if n % 4 == 2 { "c" } else { "d" }, doubled: n * 2, count: 1}
    }
    |> reduce-by --sum { |row|
      {key: row.bucket, value: {count: row.count, total: row.doubled}}
    }
  let unfused = nums
    |> par-map --jobs=8 { |n|
      {bucket: if n % 4 == 0 { "a" } else if n % 4 == 1 { "b" } else if n % 4 == 2 { "c" } else { "d" }, doubled: n * 2, count: 1}
    }
    |> reduce-by --sum --jobs=1 { |row|
      {key: row.bucket, value: {count: row.count, total: row.doubled}}
    }

  for k in unfused.keys() {
    test.eq(fused.get(k, {count: 0, total: 0}), unfused.get(k, {count: 0, total: 0}))?
  }

  test.eq(fused.keys().len(), 4)?
  test.eq(fused.get("a", {count: 0, total: 0}), {count: 12500, total: 624950000})?
}

proc test_flat_map_identity_reduce_by_matches_direct_rows() [error] {
  let nums = [0] |> range(0, 1000)
  let nested = nums
    |> par-map { |n|
      [{key: if n % 2 == 0 { "even" } else { "odd" }, count: 1, total: n}]
    }
    |> flat-map { |rows|
      rows
    }
    |> reduce-by --sum { |row|
      {key: row.key, value: {count: row.count, total: row.total}}
    }
  let direct = nums
    |> par-map { |n|
      {key: if n % 2 == 0 { "even" } else { "odd" }, count: 1, total: n}
    }
    |> reduce-by --sum { |row|
      {key: row.key, value: {count: row.count, total: row.total}}
    }

  test.eq(nested.get("even", {count: 0, total: 0}), direct.get("even", {count: 0, total: 0}))?
  test.eq(nested.get("odd", {count: 0, total: 0}), {count: 500, total: 250000})?
}

proc test_parallel_count_and_group_by_match_serial() [error] {
  # group-by must preserve encounter order within each group.
  let nums = [0] |> range(0, 20000)
  let cpar = nums |> count { if . % 2 == 0 { "even" } else { "odd" } }
  let cser = nums |> count --jobs=1 { if . % 2 == 0 { "even" } else { "odd" } }
  let gpar = nums |> group-by { . % 3 } |> sort-by .key |> map { |g| g.items }
  let gser = nums |> group-by --jobs=1 { . % 3 } |> sort-by .key |> map { |g| g.items }

  test.eq(cpar.get("even", 0), cser.get("even", 0))?
  test.eq(cpar.get("odd", 0), cser.get("odd", 0))?
  test.eq(cpar.get("even", 0), 10000)?
  test.eq(gpar, gser)?
  test.eq(gpar.len(), 3)?
}

proc test_stream_adapters_bridge_text_bytes_and_json_lines() [process, error] {
  let captured = run.text printf "%s\n" "a.txt" "b.log" ?
  let paths = captured |> text.lines() |> map { |line| Path(line) }
  let chunks = b"abcde" |> bytes.chunks(2)
  let rows = "{\"name\":\"a\",\"size\":1}\n{\"name\":\"b\",\"size\":2}\n"
    |> json.lines()
    |> sort-by .size
  let streamed = "{\"name\":\"c\",\"size\":3}\n" |> json.stream()
  let words = "one two".words()

  test.eq(paths[0].ext, "txt")?
  test.eq(paths[1].name, "b.log")?
  test.eq(chunks[0], b"ab")?
  test.eq(chunks[2], b"e")?
  test.eq(rows[1].name, "b")?
  test.eq(rows[0].size, 1)?
  test.eq(streamed[0].name, "c")?
  test.eq(words[1], "two")?
}

proc test_line_methods_and_adapters_are_lazy_sources(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "stream-lines")?
  let input = fp"${root}/input.txt"
  input.write("alpha\r\nbeta\ngamma\n")?

  test.eq((input.lines()? |> first())?, "alpha")?
  test.eq(("one\ntwo\n".lines() |> drop(1) |> first())?, "two")?
  test.eq(("red\nblue\n" |> text.lines() |> take(1))[0], "red")?
  test.eq("x\ny\n".lines().collect()[1], "y")?
  test.eq(b"a\nb\n".lines().collect().len(), 2)?
  test.eq(input.bytes_lines()?.collect()[1], b"beta")?
}

proc test_flat_map_consumes_live_streams_returned_by_blocks(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "flat-map-live-stream")?
  let left = fp"${root}/left.txt"
  let right = fp"${root}/right.txt"
  left.write("a\nb\n")?
  right.write("c\nd\n")?

  let lines = [left, right] |> flat-map { |pth|
    pth.lines()?
  }

  test.eq(lines, ["a", "b", "c", "d"])?
}

proc test_sort_by_desc_reverses_sort_order() [error] {
  let nums = [3, 1, 4, 1, 5, 9, 2, 6]
  let asc = nums |> sort-by .
  let desc = nums |> sort-by --desc .
  let words = ["banana", "apple", "cherry"] |> sort-by --desc .

  test.eq(asc[0], 1)?
  test.eq(asc[7], 9)?
  test.eq(desc[0], 9)?
  test.eq(desc[7], 1)?
  test.eq(words[0], "cherry")?
  test.eq(words[2], "apple")?
}

proc test_structured_stream_batch_count_and_argv_limits() [process, error] {
  let by_count = [1, 2, 3, 4, 5] |> batch --count=2
  let by_size = [Path("aaaa"), Path("bbbb"), Path("cccc")] |> batch --max-bytes=10

  test.eq(by_count, [[1, 2], [3, 4], [5]])?
  test.eq(by_size[0], [Path("aaaa"), Path("bbbb")])?
  test.eq(by_size[1], [Path("cccc")])?

  [Path("one"), Path("two")] |> batch --max-argv |> each { |files|
    run true @files ?
  }

  test.eq([1] |> where false |> batch --count=2 |> count(), 0)?
}

proc test_parallel_stream_stages_are_bounded_and_deterministic() [error] {
  test.eq([1, 2, 3, 4] |> par-map { |x| x * 2 }, [2, 4, 6, 8])?

  var seen: List[Str] = []
  ["a", "b"] |> each --jobs=2 { |x|
    seen = seen.push(x)
  }

  test.eq(seen, ["a", "b"])?
}

proc test_parallel_stream_preserves_filtered_order() [error] {
  test.eq([0, 1, 2, 3, 4, 5] |> where . >= 2 |> par-map --jobs=3 { |x| x * 10 }, [20, 30, 40, 50])?
}

proc test_structured_streams_walk_filter_map_collect_and_count(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "stream-walk")?
  let nested = fp"${root}/nested"
  nested.mkdir()?
  fp"${root}/a.txt".write("a")?
  fp"${nested}/b.txt".write("b")?

  let entries = fs.walk(root)
    |> where .kind == "file"
    |> collect()
  let names = entries
    |> map .name
    |> sort-by .
  let count = fs.walk(root)
    |> where .kind == "file"
    |> count()

  test.eq(names, ["a.txt", "b.txt"])?
  test.eq(count, 2)?
}

proc test_fs_walk_lazy_folding_terminals_match_eager_results(ctx: TestContext) [fs, error] {
  # count/sum/min/max/fold drive the live walk by folding one item at a time.
  let root = test.temp_dir(ctx, name: "fs-walk-fold")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/bb.txt".write("bb")?
  fp"${root}/ccc.txt".write("ccc")?

  test.eq(fs.walk(root) |> where .kind == "file" |> count(), 3)?
  test.eq(fs.walk(root) |> where .kind == "file" |> map .size |> sum(), 6)?
  test.eq((fs.walk(root) |> where .kind == "file" |> map .size |> min())?, 1)?
  test.eq((fs.walk(root) |> where .kind == "file" |> map .size |> max())?, 3)?
  test.eq(
    fs.walk(root)
      |> where .kind == "file"
      |> map .size
      |> fold(0) { |acc|
        acc + .
      },
    6,
  )?
}
