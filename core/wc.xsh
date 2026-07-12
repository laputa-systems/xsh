#!/bin/xsh
type Counts = {lines: Int, words: Int, bytes: Int}

pure count_text(text_data: Str) -> Counts {
  return {lines: text_data.count_lines(), words: text_data.count_words(), bytes: text_data.count_bytes()}
}

pure add_counts(left: Counts, right: Counts) -> Counts {
  return {lines: left.lines + right.lines, words: left.words + right.words, bytes: left.bytes + right.bytes}
}

pure max_digits(counts: List[Counts], show_lines: Bool, show_words: Bool, show_bytes: Bool) -> Int {
  var widest = 1

  for c in counts {
    if show_lines {
      let digit = f"${c.lines}".count_chars()
      widest = if digit > widest { digit } else { widest }
    }

    if show_words {
      let digit = f"${c.words}".count_chars()
      widest = if digit > widest { digit } else { widest }
    }

    if show_bytes {
      let digit = f"${c.bytes}".count_chars()
      widest = if digit > widest { digit } else { widest }
    }
  }

  return widest
}

pure format_counts(counts: Counts, show_lines: Bool, show_words: Bool, show_bytes: Bool, width: Int) -> Str {
  var cols: List[Str] = []

  if show_lines {
    cols = cols.push(tui.left_pad(f"${counts.lines}", width))
  }

  if show_words {
    cols = cols.push(tui.left_pad(f"${counts.words}", width))
  }

  if show_bytes {
    cols = cols.push(tui.left_pad(f"${counts.bytes}", width))
  }

  return cols.join(" ")
}

proc print_line(counts: Counts, label: Str, show_lines: Bool, show_words: Bool, show_bytes: Bool, width: Int) [io] {
  let body = format_counts(counts, show_lines, show_words, show_bytes, width)

  if label == "" {
    print $body
  } else {
    print f"${body} ${label}"
  }
}

proc main(...argv: List[Str]) [fs, error, io] {
  let parsed = cli.parse(
    argv,
    {
      lines: {form: "-l --lines", default: false},
      words: {form: "-w --words", default: false},
      bytes: {form: "-c --bytes", default: false},
      paths: {form: "...FILE", repeated: true},
    },
  )?

  var show_lines = parsed.lines
  var show_words = parsed.words
  var show_bytes = parsed.bytes

  if ! show_lines and ! show_words and ! show_bytes {
    show_lines = true
    show_words = true
    show_bytes = true
  }

  if parsed.paths.len() == 0 {
    let counts = count_text(io.stdin_text()?)
    let width = max_digits([counts], show_lines, show_words, show_bytes)
    let empty = ""
    print_line(counts, empty, show_lines, show_words, show_bytes, width)
    return
  }

  var counts_list: List[Counts] = []
  var labels: List[Str] = []
  var total: Counts = {lines: 0, words: 0, bytes: 0}

  for item in parsed.paths {
    var counts = {lines: 0, words: 0, bytes: 0}
    var label = item

    if item == "-" {
      counts = count_text(io.stdin_text()?)
    } else {
      let target = fp"${item}"
      counts = count_text(target.read_text()?)
    }

    if parsed.paths.len() == 1 and item == "-" {
      label = ""
    }

    total = add_counts(total, counts)
    counts_list = counts_list.push(counts)
    labels = labels.push(label)
  }

  if parsed.paths.len() > 1 {
    counts_list = counts_list.push(total)
    labels = labels.push("total")
  }

  let width = max_digits(counts_list, show_lines, show_words, show_bytes)

  for i in range(counts_list.len()) {
    print_line(counts_list[i], labels[i], show_lines, show_words, show_bytes, width)
  }
}
