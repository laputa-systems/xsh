##! The `test` and `[` expression evaluator.
##!
##! A recursive-descent port of the GNU coreutils grammar, including its
##! argument-count special cases: one to four words are classified by count
##! before the general `-o`/`-a` precedence parser runs, which is what makes
##! `test ! -a !` and `test -n -a` behave the way scripts rely on. `-a` and `-o`
##! never short-circuit, so a later invalid integer is still an error.
##!
##! Each function takes the word list and the index of the first word it may
##! read, and returns the truth value with the index of the next unread word.
##! The `raw` list carries the original bytes of operands that are not UTF-8;
##! their placeholders in the word list are restored only where GNU prints the
##! operand's bytes.

use gnu

const USAGE = """Usage: test EXPRESSION
  or:  test
  or:  [ EXPRESSION ]
  or:  [ ]
  or:  [ OPTION
Exit with the status determined by EXPRESSION.

      --help        display this help and exit
      --version     output version information and exit

An omitted EXPRESSION defaults to false.  Otherwise,
EXPRESSION is true or false and sets exit status.  It is one of:

  ( EXPRESSION )               EXPRESSION is true
  ! EXPRESSION                 EXPRESSION is false
  EXPRESSION1 -a EXPRESSION2   both EXPRESSION1 and EXPRESSION2 are true
  EXPRESSION1 -o EXPRESSION2   either EXPRESSION1 or EXPRESSION2 is true

  -n STRING            the length of STRING is nonzero
  STRING               equivalent to -n STRING
  -z STRING            the length of STRING is zero
  STRING1 = STRING2    the strings are equal
  STRING1 != STRING2   the strings are not equal
  STRING1 > STRING2    STRING1 is greater than STRING2 in the current locale
  STRING1 < STRING2    STRING1 is less than STRING2 in the current locale

  INTEGER1 -eq INTEGER2   INTEGER1 is equal to INTEGER2
  INTEGER1 -ge INTEGER2   INTEGER1 is greater than or equal to INTEGER2
  INTEGER1 -gt INTEGER2   INTEGER1 is greater than INTEGER2
  INTEGER1 -le INTEGER2   INTEGER1 is less than or equal to INTEGER2
  INTEGER1 -lt INTEGER2   INTEGER1 is less than INTEGER2
  INTEGER1 -ne INTEGER2   INTEGER1 is not equal to INTEGER2

  FILE1 -ef FILE2   FILE1 and FILE2 have the same device and inode numbers
  FILE1 -nt FILE2   FILE1 is newer (modification date) than FILE2
  FILE1 -ot FILE2   FILE1 is older than FILE2

  -b FILE     FILE exists and is block special
  -c FILE     FILE exists and is character special
  -d FILE     FILE exists and is a directory
  -e FILE     FILE exists
  -f FILE     FILE exists and is a regular file
  -g FILE     FILE exists and its set-group-ID bit is set
  -G FILE     FILE exists and is owned by the effective group ID
  -h FILE     FILE exists and is a symbolic link (same as -L)
  -k FILE     FILE exists and has its sticky bit set
  -L FILE     FILE exists and is a symbolic link (same as -h)
  -N FILE     FILE exists and has been modified since it was last read
  -O FILE     FILE exists and is owned by the effective user ID
  -p FILE     FILE exists and is a named pipe
  -r FILE     FILE exists and the user has read access
  -s FILE     FILE exists and has a size greater than zero
  -S FILE     FILE exists and is a socket
  -t FD       file descriptor FD is opened on a terminal
  -u FILE     FILE exists and its set-user-ID bit is set
  -w FILE     FILE exists and the user has write access
  -x FILE     FILE exists and the user has execute (or search) access

Except for -h and -L, all FILE-related tests dereference symbolic links.
Beware that parentheses need to be escaped (e.g., by backslashes) for shells.
INTEGER may also be -l STRING, which evaluates to the length of STRING.

Binary -a and -o are ambiguous.  Use 'test EXPR1 && test EXPR2'
or 'test EXPR1 || test EXPR2' instead.

'[' honors --help and --version, but 'test' treats them as STRINGs.

NOTE: your shell may have its own version of test and/or [, which usually
supersedes the version described here.  Please refer to your shell's
documentation for details about the options it supports.
"""

type Eval = {value: Bool, pos: Int}

type Integer = {ok: Bool, negative: Bool, digits: Str}

type Identity = {euid: Int, egid: Int, groups: List[Int]}

const UNARY_OPS = [
  "-b",
  "-c",
  "-d",
  "-e",
  "-f",
  "-g",
  "-G",
  "-h",
  "-k",
  "-L",
  "-n",
  "-N",
  "-O",
  "-p",
  "-r",
  "-s",
  "-S",
  "-t",
  "-u",
  "-w",
  "-x",
  "-z",
]

const INTEGER_OPS = ["-eq", "-ne", "-lt", "-le", "-gt", "-ge"]
const FILE_OPS = ["-nt", "-ot", "-ef"]
const STRING_OPS = ["=", "==", "!=", "<", ">"]

pure is_binop(word: Str) -> Bool {
  word in INTEGER_OPS or word in FILE_OPS or word in STRING_OPS
}

proc syntax_error(message: Str) [process, env] -> Unit {
  gnu.error(message)
  exit 2
}

# `missing argument after LAST-WORD`, the error for running out of words.
proc beyond(argv: List[Str]) [process, env] -> Unit {
  syntax_error(f"missing argument after {gnu.quote_value(argv[-1])}")
}

pure blank(text: Str) -> Bool {
  text == " " or text == "\t" or text == "\n" or text == "\u{b}" or text == "\u{c}" or text == "\r"
}

# An integer operand as GNU reads it: surrounding blanks, an optional sign,
# and decimal digits of any length. Leading zeros and a sign on zero vanish.
pure parse_integer(text: Str) -> Integer {
  let invalid = {ok: false, negative: false, digits: ""}
  var start = 0
  var end = text.byte_len()

  while start < end and blank(text.byte_slice(start, length: 1)) {
    start += 1
  }

  while end > start and blank(text.byte_slice(end - 1, length: 1)) {
    end -= 1
  }

  let body = text.byte_slice(start, length: end - start)
  let negative = body.starts_with("-")
  let digits = if negative or body.starts_with("+") { body.byte_slice(1) } else { body }

  return invalid when digits == "" or ! rx"^[0-9]+$".matches(digits)

  var first = 0

  while first < digits.byte_len() - 1 and digits.byte_slice(first, length: 1) == "0" {
    first += 1
  }

  let trimmed = digits.byte_slice(first)

  {ok: true, negative: negative and trimmed != "0", digits: trimmed}
}

# -1, 0, or 1 for LEFT compared with RIGHT as integers.
pure compare_integers(left: Integer, right: Integer) -> Int {
  return -1 when left.negative and ! right.negative
  return 1 when ! left.negative and right.negative

  let flip = if left.negative { -1 } else { 1 }
  let by_width = left.digits.byte_len() - right.digits.byte_len()

  return flip when by_width > 0
  return 0 - flip when by_width < 0
  return flip when left.digits > right.digits
  return 0 - flip when left.digits < right.digits

  0
}

# The integer operand at `text`. An invalid one is quoted from its original
# bytes, so an undecodable operand prints the way GNU prints it.
proc integer_operand(text: Str, raw: List[gnu.RawArgument]) [process, env] -> Integer {
  let number = parse_integer(text)

  if ! number.ok {
    syntax_error(f"invalid integer {gnu.quote_value_bytes(gnu.argument_bytes(text, raw))}")
  }

  number
}

# The integer an operand stands for; `-l STRING` is the length of the original
# bytes of STRING.
proc integer_value(text: Str, length_of: Bool, raw: List[gnu.RawArgument]) [process, env] -> Integer {
  return parse_integer(f"{gnu.argument_bytes(text, raw).len()}") when length_of

  integer_operand(text, raw)
}

proc identity() [process, error] -> Result[Identity] {
  let me = unix.id()?

  Ok({euid: me.euid, egid: me.egid, groups: [entry.gid for entry in me.groups]})
}

# Metadata of NAME, following symlinks unless `follow` is false. A missing or
# dangling name is an error.
proc stat_name(name: Bytes, follow: Bool) [fs, error] -> Result[FsEntry] {
  let target = Path.parse_bytes(name)?

  return target.metadata() when ! follow

  target.resolve()?.metadata()
}

# Access of the calling process to a file, from its permission bits. `bit` is
# 4, 2, or 1 for read, write, and execute or search.
proc permitted(entry: FsEntry, bit: Int) [process, error] -> Result[Bool] {
  let who = identity()?
  let directory = entry.kind == "dir"

  if who.euid == 0 {
    return Ok(bit != 1 or directory or entry.mode / 64 % 2 == 1 or entry.mode / 8 % 2 == 1 or entry.mode % 2 == 1)
  }

  let shift = if entry.uid == who.euid { 64 } else if entry.gid == who.egid or entry.gid in who.groups { 8 } else { 1 }

  Ok(entry.mode / shift % 8 / bit % 2 == 1)
}

proc is_terminal(text: Str, raw: List[gnu.RawArgument]) [process, env] -> Bool {
  let fd = integer_operand(text, raw)

  return false when fd.negative or fd.digits.byte_len() > 9

  if let Ok(_) = unix.tty_attrs(fd.digits.parse_int() ?? 0) {
    return true
  }

  false
}

# `-X NAME` file and string tests.
proc unary(op: Str, operand: Str, raw: List[gnu.RawArgument]) [fs, process, env, error] -> Result[Bool] {
  return Ok(operand != "") when op == "-n"
  return Ok(operand == "") when op == "-z"
  return Ok(is_terminal(operand, raw)) when op == "-t"

  let follow = op != "-h" and op != "-L"

  return Ok(false) when operand == ""

  guard let entry = stat_name(gnu.argument_bytes(operand, raw), follow) else {
    return Ok(false)
  }

  let kind = entry.mode / 4096 % 16
  let who = identity()?

  return Ok(true) when op == "-e"
  return Ok(kind == 6) when op == "-b"
  return Ok(kind == 2) when op == "-c"
  return Ok(kind == 4) when op == "-d"
  return Ok(kind == 8) when op == "-f"
  return Ok(kind == 10) when op == "-h" or op == "-L"
  return Ok(kind == 1) when op == "-p"
  return Ok(kind == 12) when op == "-S"
  return Ok(entry.mode / 1024 % 2 == 1) when op == "-g"
  return Ok(entry.mode / 2048 % 2 == 1) when op == "-u"
  return Ok(entry.mode / 512 % 2 == 1) when op == "-k"
  return Ok(entry.size > 0) when op == "-s"
  return Ok(entry.uid == who.euid) when op == "-O"
  return Ok(entry.gid == who.egid) when op == "-G"
  return Ok(entry.modified > entry.accessed) when op == "-N"
  return permitted(entry, 4) when op == "-r"
  return permitted(entry, 2) when op == "-w"

  permitted(entry, 1)
}

# The unary test at `at`, selected by the second character of the word like
# GNU does: an unknown operator is a syntax error even without an operand, and
# a known one needs the next word.
proc unary_at(argv: List[Str], raw: List[gnu.RawArgument], at: Int) [fs, process, env, error] -> Result[Eval] {
  let op = argv[at].byte_slice(0, length: 2)

  if ! (op in UNARY_OPS) {
    syntax_error(f"{gnu.quote_value(argv[at])}: unary operator expected")
  }

  if at + 1 >= argv.len() {
    beyond(argv)
  }

  Ok({value: unary(op, argv[at + 1], raw)?, pos: at + 2})
}

# GNU treats a two-character word starting with `-` as a switch except `-a`
# and `-o`, which stay strings inside a longer expression.
pure is_switch(word: Str) -> Bool {
  word.byte_len() == 2 and word.starts_with("-")
}

# Whether two names are one file: equal device and inode after following
# symlinks, so a hard link or a symlink to the same file matches. A name that
# does not resolve is never the same file as another.
proc same_file(left: Bytes, right: Bytes) [fs, error] -> Result[Bool] {
  let left_path = Path.parse_bytes(left)?
  let right_path = Path.parse_bytes(right)?

  guard let left_entry = fs.stat(left_path, follow_symlinks: true) else {
    return Ok(false)
  }

  guard let right_entry = fs.stat(right_path, follow_symlinks: true) else {
    return Ok(false)
  }

  Ok(left_entry.dev == right_entry.dev and left_entry.ino == right_entry.ino)
}

proc file_compare(op: Str, left: Bytes, right: Bytes) [fs, process, env, error] -> Result[Bool] {
  return same_file(left, right) when op == "-ef"

  var left_time = 0
  var right_time = 0
  var left_found = false
  var right_found = false

  if let Ok(entry) = stat_name(left, true) {
    left_found = true
    left_time = entry.modified
  }

  if let Ok(entry) = stat_name(right, true) {
    right_found = true
    right_time = entry.modified
  }

  return Ok(left_found and (! right_found or left_time > right_time)) when op == "-nt"

  Ok(right_found and (! left_found or left_time < right_time))
}

# `LEFT OP RIGHT` starting at the left operand, or at `-l` when `length_left`.
# GNU also reads `OP -l STRING` on the right when two or more words follow OP;
# the operands stay where the unshifted layout puts them, so `a = -l b` compares
# `a` with the literal `-l`.
proc binary(argv: List[Str], raw: List[gnu.RawArgument], at: Int, length_left: Bool) [fs, process, env, error] -> Result[Eval] {
  let op_at = if length_left { at + 2 } else { at + 1 }
  let op = argv[op_at]
  let length_right = op_at < argv.len() - 2 and argv[op_at + 1] == "-l"
  let consumed = (if length_left { 1 } else { 0 }) + (if length_right { 1 } else { 0 }) + 3
  let next = at + consumed
  let left = argv[op_at - 1]
  let right = argv[op_at + 1]

  if op in INTEGER_OPS {
    let left_number = integer_value(left, length_left, raw)
    let right_number = integer_value(if length_right { argv[op_at + 2] } else { right }, length_right, raw)
    let order = compare_integers(left_number, right_number)
    let held = if op == "-eq" {
      order == 0
    } else if op == "-ne" {
      order != 0
    } else if op == "-lt" {
      order < 0
    } else if op == "-le" {
      order <= 0
    } else if op == "-gt" {
      order > 0
    } else {
      order >= 0
    }

    return Ok({value: held, pos: next})
  }

  if op in FILE_OPS {
    if length_left or length_right {
      syntax_error(f"{op} does not accept -l")
    }

    return Ok({value: file_compare(op, gnu.argument_bytes(left, raw), gnu.argument_bytes(right, raw))?, pos: next})
  }

  # Equality compares the original bytes so two identical undecodable operands
  # match. Ordering stays on text because `Bytes` has no ordering operator.
  return Ok({value: gnu.argument_bytes(left, raw) == gnu.argument_bytes(right, raw), pos: next}) when op == "=" or op == "=="

  return Ok({value: gnu.argument_bytes(left, raw) != gnu.argument_bytes(right, raw), pos: next}) when op == "!="

  return Ok({value: left < right, pos: next}) when op == "<"

  Ok({value: left > right, pos: next})
}

proc two_arguments(argv: List[Str], raw: List[gnu.RawArgument], at: Int) [fs, process, env, error] -> Result[Eval] {
  return Ok({value: argv[at + 1] == "", pos: at + 2}) when argv[at] == "!"

  return unary_at(argv, raw, at) when is_switch(argv[at])

  beyond(argv)
  Ok({value: false, pos: at})
}

proc three_arguments(argv: List[Str], raw: List[gnu.RawArgument], at: Int) [fs, process, env, error] -> Result[Eval] {
  return binary(argv, raw, at, false) when is_binop(argv[at + 1])

  if argv[at] == "!" {
    let inner = two_arguments(argv, raw, at + 1)?

    return Ok({value: ! inner.value, pos: inner.pos})
  }

  if argv[at] == "(" and argv[at + 2] == ")" {
    return Ok({value: argv[at + 1] != "", pos: at + 3})
  }

  return expression(argv, raw, at) when argv[at + 1] == "-a" or argv[at + 1] == "-o"

  syntax_error(f"{gnu.quote_value(argv[at + 1])}: binary operator expected")
  Ok({value: false, pos: at})
}

# GNU classifies the words by count: one to four words never reach the
# general parser unless they contain a boolean operator.
proc posixtest(argv: List[Str], raw: List[gnu.RawArgument], at: Int, count: Int) [fs, process, env, error] -> Result[Eval] {
  return Ok({value: argv[at] != "", pos: at + 1}) when count == 1

  return two_arguments(argv, raw, at) when count == 2

  return three_arguments(argv, raw, at) when count == 3

  if count == 4 {
    if argv[at] == "!" {
      let inner = three_arguments(argv, raw, at + 1)?

      return Ok({value: ! inner.value, pos: inner.pos})
    }

    if argv[at] == "(" and argv[at + 3] == ")" {
      let inner = two_arguments(argv, raw, at + 1)?

      return Ok({value: inner.value, pos: inner.pos + 1})
    }
  }

  expression(argv, raw, at)
}

proc term(argv: List[Str], raw: List[gnu.RawArgument], start: Int) [fs, process, env, error] -> Result[Eval] {
  var at = start
  var negated = false

  while at < argv.len() and argv[at] == "!" {
    at += 1
    negated = ! negated

    if at >= argv.len() {
      beyond(argv)
    }
  }

  if at >= argv.len() {
    beyond(argv)
  }

  var result = {value: false, pos: at}

  if argv[at] == "(" {
    at += 1

    if at >= argv.len() {
      beyond(argv)
    }

    var count = 1

    while at + count < argv.len() and argv[at + count] != ")" {
      if count == 4 {
        count = argv.len() - at
        break
      }

      count += 1
    }

    let inner = posixtest(argv, raw, at, count)?

    if inner.pos >= argv.len() {
      syntax_error("')' expected")
    }

    if argv[inner.pos] != ")" {
      syntax_error(f"')' expected, found {gnu.quote_value(argv[inner.pos])}")
    }

    result = {value: inner.value, pos: inner.pos + 1}
  } else if argv.len() - at >= 4 and argv[at] == "-l" and is_binop(argv[at + 2]) {
    result = binary(argv, raw, at, true)?
  } else if argv.len() - at >= 3 and is_binop(argv[at + 1]) {
    result = binary(argv, raw, at, false)?
  } else if is_switch(argv[at]) and argv[at] != "-a" and argv[at] != "-o" {
    result = unary_at(argv, raw, at)?
  } else {
    result = {value: argv[at] != "", pos: at + 1}
  }

  Ok({value: negated != result.value, pos: result.pos})
}

proc conjunction(argv: List[Str], raw: List[gnu.RawArgument], start: Int) [fs, process, env, error] -> Result[Eval] {
  var at = start
  var value = true

  while true {
    let item = term(argv, raw, at)?

    value = item.value and value
    at = item.pos

    break when at >= argv.len() or argv[at] != "-a"

    at += 1
  }

  Ok({value: value, pos: at})
}

proc expression(argv: List[Str], raw: List[gnu.RawArgument], start: Int) [fs, process, env, error] -> Result[Eval] {
  var at = start
  var value = false

  if at >= argv.len() {
    beyond(argv)
  }

  while true {
    let item = conjunction(argv, raw, at)?

    value = item.value or value
    at = item.pos

    break when at >= argv.len() or argv[at] != "-o"

    at += 1
  }

  Ok({value: value, pos: at})
}

## Evaluate the words of a `test` command line. Ends the applet with the
## exit status: 0 true, 1 false, 2 for a syntax error.
export proc evaluate(argv: List[Bytes]) [fs, process, env, error, io] -> Unit {
  let prepared = gnu.prepare_arguments(argv)
  let bracket = gnu.prog() == "["
  var words = prepared.text

  if bracket {
    if words.len() == 1 and words[0] == "--help" {
      gnu.help(USAGE)
      return
    }

    if words.len() == 1 and words[0] == "--version" {
      gnu.version("[")
      return
    }

    if words.is_empty() or words[-1] != "]" {
      syntax_error("missing ']'")
    }

    words = words[0..words.len() - 1]
  }

  if words.is_empty() {
    exit 1
  }

  let outcome = posixtest(words, prepared.raw, 0, words.len())?

  if outcome.pos != words.len() {
    syntax_error(f"extra argument {gnu.quote_value_bytes(gnu.argument_bytes(words[outcome.pos], prepared.raw))}")
  }

  if ! outcome.value {
    exit 1
  }
}
