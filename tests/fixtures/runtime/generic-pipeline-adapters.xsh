pure lines(marker, input) { let _ = marker; input |> text.lines() }
pure chunks(marker, input, size: Int) { let _ = marker; input |> bytes.chunks(size) }
pure json_lines(marker, input) -> List[Any] { let _ = marker; input |> json.lines() }
pure json_stream(marker, input) -> List[Any] { let _ = marker; input |> json.stream() }

pure json_line_count(marker, input) { let _ = marker; let rows = input |> json.lines(); rows.len() }
pure json_stream_count(marker, input) { let _ = marker; let rows = input |> json.stream(); rows.len() }

assert lines(7, "one\r\n\ntwo\n") == ["one", "", "two"]
assert lines("independent", "") == []
assert chunks(true, b"abcde", 2) == [b"ab", b"cd", b"e"]
assert chunks(7, b"", 3) == []
let rows = json_lines(7, "{\"name\":\"one\"}\n\n{\"name\":\"two\"}\n")
assert rows.len() == 2
assert json.get(rows[0], ["name"])? == "one"
let stream_rows = json_stream("independent", "{\"ok\":true}\n{\"ok\":false}\n")
assert stream_rows.len() == 2
assert json.get(stream_rows[1], ["ok"])? == false
assert json_line_count(true, "1\ntrue\n") == 2
assert json_stream_count(7, "1\n\ntrue\n") == 2
print "pipeline adapters preserved"
