# A small stand-in for ed(1) for the reference container, which has none.
# GNU patch runs `ed - FILE` and feeds it the script followed by `w` and `q`.
# Only the commands that diff -e produces are understood: addresses made of
# numbers, `.`, and `$`; a, i, c, d, s/re/text/[g]; w; q.
function addr(text) {
  if (text == ".") return cur
  if (text == "$") return n
  return text + 0
}
function splice(from, to, count,   i, tail) {
  # Replace lines from..to of buf with added[1..count].
  delete tailbuf
  tn = 0
  for (i = to + 1; i <= n; i++) tailbuf[++tn] = buf[i]
  for (i = from; i <= n; i++) delete buf[i]
  n = from - 1
  for (i = 1; i <= count; i++) buf[++n] = added[i]
  for (i = 1; i <= tn; i++) buf[++n] = tailbuf[i]
}
BEGIN {
  file = ARGV[2]
  ARGC = 1
  n = 0
  while ((getline line < file) > 0) buf[++n] = line
  close(file)
  cur = n
  mode = ""
}
{
  if (mode == "text") {
    if ($0 == ".") {
      if (cmd == "a") splice(first + 1, first, count)
      else if (cmd == "i") splice(first, first - 1, count)
      else if (cmd == "c") splice(first, last, count)
      cur = first - 1 + count + (cmd == "a" ? 1 : 0)
      if (cmd == "a") cur = first + count
      mode = ""
    } else {
      added[++count] = $0
    }
    next
  }
  line = $0
  if (line == "w" || line == "q") {
    if (line == "w") {
      printf "" > file
      for (i = 1; i <= n; i++) print buf[i] > file
      close(file)
    }
    next
  }
  if (match(line, /^([0-9.$]*)(,([0-9.$]+))?([aicds])/)) {
    a1 = substr(line, 1, RLENGTH)
    cmd = substr(a1, length(a1))
    rest = substr(line, RLENGTH + 1)
    spec = substr(a1, 1, length(a1) - 1)
    split(spec, parts, ",")
    first = parts[1] == "" ? cur : addr(parts[1])
    last = parts[2] == "" ? first : addr(parts[2])
    if (cmd == "a" || cmd == "i" || cmd == "c") {
      mode = "text"
      count = 0
      delete added
      if (cmd == "c") {
        # `c` deletes the range, then inserts the text there.
      }
    } else if (cmd == "d") {
      count = 0
      delete added
      splice(first, last, 0)
      cur = first <= n ? first : n
    } else if (cmd == "s") {
      delim = substr(rest, 1, 1)
      m = split(rest, sp, delim)
      pat = sp[2]; rep = sp[3]; flags = sp[4]
      for (i = first; i <= last; i++) {
        if (flags ~ /g/) gsub(pat, rep, buf[i]); else sub(pat, rep, buf[i])
      }
      cur = last
    }
    next
  }
  print "?" > "/dev/stderr"
  exit 1
}
