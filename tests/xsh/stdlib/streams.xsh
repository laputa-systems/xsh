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
