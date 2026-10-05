#!/bin/xsh
use lib.gnu

const USAGE = r"""Usage: stty [-F DEVICE | --file=DEVICE] [SETTING]...
  or:  stty [-F DEVICE | --file=DEVICE] [-a|--all]
  or:  stty [-F DEVICE | --file=DEVICE] [-g|--save]
Print or change terminal characteristics.

Mandatory arguments to long options are mandatory for short options too.
  -a, --all          print all current settings in human-readable form
  -g, --save         print all current settings in a stty-readable form
  -F, --file=DEVICE  open and use the specified DEVICE instead of stdin
      --help        display this help and exit
      --version     output version information and exit

Optional - before SETTING indicates negation.  An * marks non-POSIX
settings.  The underlying system defines which settings are available.

Special characters:
 * discard CHAR  CHAR will toggle discarding of output
   eof CHAR      CHAR will send an end of file (terminate the input)
   eol CHAR      CHAR will end the line
 * eol2 CHAR     alternate CHAR for ending the line
   erase CHAR    CHAR will erase the last character typed
   intr CHAR     CHAR will send an interrupt signal
   kill CHAR     CHAR will erase the current line
 * lnext CHAR    CHAR will enter the next character quoted
   quit CHAR     CHAR will send a quit signal
 * rprnt CHAR    CHAR will redraw the current line
   start CHAR    CHAR will restart the output after stopping it
   stop CHAR     CHAR will stop the output
   susp CHAR     CHAR will send a terminal stop signal
 * swtch CHAR    CHAR will switch to a different shell layer
 * werase CHAR   CHAR will erase the last word typed

Special settings:
   N             set the input and output speeds to N bauds
 * cols N        tell the kernel that the terminal has N columns
 * columns N     same as cols N
 * [-]drain      wait for transmission before applying settings (on by default)
   ispeed N      set the input speed to N
 * line N        use line discipline N
   min N         with -icanon, set N characters minimum for a completed read
   ospeed N      set the output speed to N
 * rows N        tell the kernel that the terminal has N rows
 * size          print the number of rows and columns according to the kernel
   speed         print the terminal speed
   time N        with -icanon, set read timeout of N tenths of a second

Control settings:
   [-]clocal     disable modem control signals
   [-]cread      allow input to be received
 * [-]crtscts    enable RTS/CTS handshaking
   csN           set character size to N bits, N in [5..8]
   [-]cstopb     use two stop bits per character (one with '-')
   [-]hup        send a hangup signal when the last process closes the tty
   [-]hupcl      same as [-]hup
   [-]parenb     generate parity bit in output and expect parity bit in input
   [-]parodd     set odd parity (or even parity with '-')
 * [-]cmspar     use "stick" (mark/space) parity

Input settings:
   [-]brkint     breaks cause an interrupt signal
   [-]icrnl      translate carriage return to newline
   [-]ignbrk     ignore break characters
   [-]igncr      ignore carriage return
   [-]ignpar     ignore characters with parity errors
 * [-]imaxbel    beep and do not flush a full input buffer on a character
   [-]inlcr      translate newline to carriage return
   [-]inpck      enable input parity checking
   [-]istrip     clear high (8th) bit of input characters
 * [-]iutf8      assume input characters are UTF-8 encoded
 * [-]iuclc      translate uppercase characters to lowercase
 * [-]ixany      let any character restart output, not only start character
   [-]ixoff      enable sending of start/stop characters
   [-]ixon       enable XON/XOFF flow control
   [-]parmrk     mark parity errors (with a 255-0-character sequence)
   [-]tandem     same as [-]ixoff

Output settings:
 * bsN           backspace delay style, N in [0..1]
 * crN           carriage return delay style, N in [0..3]
 * ffN           form feed delay style, N in [0..1]
 * nlN           newline delay style, N in [0..1]
 * [-]ocrnl      translate carriage return to newline
 * [-]ofdel      use delete characters for fill instead of NUL characters
 * [-]ofill      use fill (padding) characters instead of timing for delays
 * [-]olcuc      translate lowercase characters to uppercase
 * [-]onlcr      translate newline to carriage return-newline
 * [-]onlret     newline performs a carriage return
 * [-]onocr      do not print carriage returns in the first column
   [-]opost      postprocess output
 * tabN          horizontal tab delay style, N in [0..3]
 * tabs          same as tab0
 * -tabs         same as tab3
 * vtN           vertical tab delay style, N in [0..1]

Local settings:
   [-]crterase   echo erase characters as backspace-space-backspace
 * crtkill       kill all line by obeying the echoprt and echoe settings
 * -crtkill      kill all line by obeying the echoctl and echok settings
 * [-]ctlecho    echo control characters in hat notation ('^c')
   [-]echo       echo input characters
 * [-]echoctl    same as [-]ctlecho
   [-]echoe      same as [-]crterase
   [-]echok      echo a newline after a kill character
 * [-]echoke     same as [-]crtkill
   [-]echonl     echo newline even if not echoing other characters
 * [-]echoprt    echo erased characters backward, between '\' and '/'
 * [-]extproc    enable "LINEMODE"; useful with high latency links
 * [-]flusho     discard output
   [-]icanon     enable special characters: erase, kill, werase, rprnt
   [-]iexten     enable non-POSIX special characters
   [-]isig       enable interrupt, quit, and suspend special characters
   [-]noflsh     disable flushing after interrupt and quit special characters
 * [-]prterase   same as [-]echoprt
 * [-]tostop     stop background jobs that try to write to the terminal
 * [-]xcase      with icanon, escape with '\' for uppercase characters

Combination settings:
 * [-]LCASE      same as [-]lcase
   cbreak        same as -icanon
   -cbreak       same as icanon
   cooked        same as brkint ignpar istrip icrnl ixon opost isig
                 icanon, eof and eol characters to their default values
   -cooked       same as raw
   crt           same as echoe echoctl echoke
   dec           same as echoe echoctl echoke -ixany intr ^c erase 0177
                 kill ^u
 * [-]decctlq    same as [-]ixany
   ek            erase and kill characters to their default values
   evenp         same as parenb -parodd cs7
   -evenp        same as -parenb cs8
 * [-]lcase      same as xcase iuclc olcuc
   litout        same as -parenb -istrip -opost cs8
   -litout       same as parenb istrip opost cs7
   nl            same as -icrnl -onlcr
   -nl           same as icrnl -inlcr -igncr onlcr -ocrnl -onlret
   oddp          same as parenb parodd cs7
   -oddp         same as -parenb cs8
   [-]parity     same as [-]evenp
   pass8         same as -parenb -istrip cs8
   -pass8        same as parenb istrip cs7
   raw           same as -ignbrk -brkint -ignpar -parmrk -inpck -istrip
                 -inlcr -igncr -icrnl -ixon -ixoff -icanon -opost
                 -isig -iuclc -ixany -imaxbel -xcase min 1 time 0
   -raw          same as cooked
   sane          same as cread -ignbrk brkint -inlcr -igncr icrnl
                 icanon iexten echo echoe echok -echonl -noflsh
                 -ixoff -iutf8 -iuclc -ixany imaxbel -xcase -olcuc -ocrnl
                 opost -ofill onlcr -onocr -onlret nl0 cr0 tab0 bs0 vt0 ff0
                 isig -tostop -ofdel -echoprt echoctl echoke -extproc -flusho,
                 all special characters to their default values

Handle the tty line connected to standard input.  Without arguments,
prints baud rate, line discipline, and deviations from stty sane.  In
settings, CHAR is taken literally, or coded as in ^c, 0x37, 0177 or
127; special values ^- or undef used to disable special characters.
"""

# One parsed setting. `kind` is flag, char, combo, ispeed, ospeed, both, rows,
# cols, size, speed, line, drain, saved or number (min, time); `name` is the
# setting word without a leading `-`, `text` its argument, `number` its parsed
# integer argument.
type Setting = {kind: Str, name: Str, reversed: Bool, text: Str, number: Int}

type Parsed = {all: Bool, save: Bool, file: Str?, help: Bool, version: Bool, settings: List[Str], marked: Bool}

type Integer = {status: Str, value: Int}

type Device = {fd: Int, name: Str, opened: Bool}

# Linux's glibc and musl `termios` carry 32 control characters; the kernel
# knows the first 19, so the rest read and save as zero.
const NCCS = 32

const KERNEL_CHARS = 19

const CBAUD_MASK = 4111

const CIBAUD_SHIFT = 65536

const CIBAUD_MASK = 269418496

const OVERFLOW_TEXT = "Value too large for defined data type"

const CONTROL_FLAGS = ["parenb", "parodd", "cmspar", "cs5", "cs6", "cs7", "cs8", "hupcl", "cstopb", "cread", "clocal", "crtscts"]

const INPUT_FLAGS = ["ignbrk", "brkint", "ignpar", "parmrk", "inpck", "istrip", "inlcr", "igncr", "icrnl", "ixon", "ixoff", "iuclc", "ixany", "imaxbel", "iutf8"]

const OUTPUT_FLAGS = ["opost", "olcuc", "ocrnl", "onlcr", "onocr", "onlret", "ofill", "ofdel", "nl0", "nl1", "cr0", "cr1", "cr2", "cr3", "tab0", "tab1", "tab2", "tab3", "bs0", "bs1", "vt0", "vt1", "ff0", "ff1"]

const LOCAL_FLAGS = ["isig", "icanon", "iexten", "echo", "echoe", "echok", "echonl", "noflsh", "xcase", "tostop", "echoprt", "echoctl", "echoke", "flusho", "extproc"]

const CHAR_NAMES = ["intr", "quit", "erase", "kill", "eof", "eol", "eol2", "swtch", "start", "stop", "susp", "rprnt", "werase", "lnext", "discard"]

# Flags `sane` clears: shown by plain `stty` when they are set. The flags
# `sane` sets are the table's `sane` entries and show as `-name` when clear.
const SANE_CLEARED = ["ignbrk", "inlcr", "igncr", "echonl", "noflsh", "ixoff", "iutf8", "iuclc", "ixany", "xcase", "olcuc", "ocrnl", "ofill", "onocr", "onlret", "tostop", "ofdel", "echoprt", "extproc", "flusho"]

# The flag a second spelling names.
pure alias_of(word: Str) -> Str {
  return "hupcl" when word == "hup"
  return "ixoff" when word == "tandem"
  return "echoe" when word == "crterase"
  return "echoprt" when word == "prterase"
  return "echoctl" when word == "ctlecho"
  return "echoke" when word == "crtkill"

  word
}

const COMBOS_NEGATABLE = ["LCASE", "lcase", "cbreak", "cooked", "decctlq", "evenp", "litout", "nl", "oddp", "parity", "pass8", "raw", "tabs"]

const COMBOS_PLAIN = ["crt", "dec", "ek", "sane"]

const PRINTABLE = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{{|}}~"

pure char_text(code: Int) -> Str {
  PRINTABLE.byte_slice(code - 32, length: 1)
}

pure hex(value: Int) -> Str {
  return "0" when value == 0

  var rest = value
  var out = ""
  let digits = "0123456789abcdef"

  while rest > 0 {
    out = f"{digits.byte_slice(rest % 16, length: 1)}{out}"
    rest = rest / 16
  }

  out
}

pure hex_digit(code: Int) -> Int {
  return code - 48 when code >= 48 and code <= 57
  return code - 87 when code >= 97 and code <= 102
  return code - 55 when code >= 65 and code <= 70

  -1
}

# C `strtoul` with base 0 on the whole text: optional blanks and `+`, then
# `0x` hex, leading-`0` octal or decimal digits and nothing after them. A
# negative number, stray text or no digits is `invalid`; a value beyond 2^62
# is `overflow`. Size suffixes (`1k`) are not accepted.
pure parse_c_integer(text: Str) -> Integer {
  let total = text.byte_len()
  var at = 0

  while at < total and (text.byte_at(at) == 32 or ((text.byte_at(at) ?? 0) >= 9 and (text.byte_at(at) ?? 0) <= 13)) {
    at += 1
  }

  if at < total and text.byte_at(at) == 43 {
    at += 1
  }

  var base = 10

  if at + 2 < total + 0 and text.byte_at(at) == 48 and (text.byte_at(at + 1) == 120 or text.byte_at(at + 1) == 88) and hex_digit(text.byte_at(at + 2) ?? 0) >= 0 {
    base = 16
    at += 2
  } else if at < total and text.byte_at(at) == 48 {
    base = 8
  }

  var value = 0
  var digits = 0
  var overflow = false

  while at < total {
    let digit = hex_digit(text.byte_at(at) ?? 0)

    return {status: "invalid", value: 0} when digit < 0 or digit >= base

    if value > 1152921504606846976 {
      overflow = true
    } else {
      value = value * base + digit
    }

    digits += 1
    at += 1
  }

  return {status: "invalid", value: 0} when digits == 0
  return {status: "overflow", value: 0} when overflow

  {status: "ok", value: value}
}

# GNU `integer_arg`: a number no larger than `max`, or the diagnostic and exit
# status 1.
proc integer_arg(text: Str, max: Int) [process, env] -> Int {
  let parsed = parse_c_integer(text)

  if parsed.status == "invalid" {
    gnu.error(f"invalid integer argument: {gnu.quote_value(text)}")
    exit 1
  }

  if parsed.status == "overflow" or parsed.value > max {
    gnu.error(f"invalid integer argument: {gnu.quote_value(text)}: {OVERFLOW_TEXT}")
    exit 1
  }

  parsed.value
}

# A control character argument: a literal character, `^C`, `^-` or `undef`,
# or a number up to 255.
proc char_value(text: Str) [process, env] -> Int {
  let total = text.byte_len()

  return text.byte_at(0) ?? 0 when total <= 1

  return 0 when text == "^-" or text == "undef"

  if text.byte_slice(0, length: 1) == "^" {
    let second = text.byte_at(1) ?? 0

    return 127 when second == 63

    return second % 32
  }

  integer_arg(text, 255)
}

# GNU `string_to_baud` on the supported speeds: blanks before the number,
# a leading `+`, a trailing `.`, a fraction rounded to the nearest whole
# speed (halves to even) and the names `exta` and `extb`. Returns the speed or
# -1.
pure baud_value(text: Str, speeds: List[Int]) -> Int {
  return 19200 when text == "exta"
  return 38400 when text == "extb"

  let trimmed_end = text.trim()
  let lead = text.byte_slice(0, length: 1)

  return -1 when text.ends_with(" ") or text.ends_with("\t")
  return -1 when trimmed_end.starts_with("-") or trimmed_end.starts_with("++") or trimmed_end.lower().find("e") != null
  return -1 when trimmed_end.split(".").len() > 2

  var number = trimmed_end
  number = if number.starts_with("+") { number.byte_slice(1) } else { number }
  number = if number.ends_with(".") { number.byte_slice(0, length: number.byte_len() - 1) } else { number }

  let parts = number.split(".")
  let whole = parts[0]

  return -1 when whole == "" or ! rx"^[0-9]+$".matches(whole)

  var value = whole.parse_int_decimal() ?? -1

  return -1 when value < 0 or value > 4294967295

  if let [_, fraction] = parts {

    return -1 when fraction == "" or ! rx"^[0-9]+$".matches(fraction)

    let first = fraction.byte_at(0) ?? 48
    let rest = fraction.byte_slice(1)

    if first > 53 {
      value += 1
    } else if first == 53 {
      if rest.delete("0") != "" {
        value += 1
      } else {
        value += value % 2
      }
    }
  }

  if value in speeds { value } else { -1 }
}

# The speed a `termios` baud code names: codes 0 to 15 and then the extended
# codes `0x1001`.. index the speed table.
pure speed_of_code(code: Int, speeds: List[Int]) -> Int {
  let index = if code <= 15 { code } else { 15 + code - 4096 }

  if index >= 0 and index < speeds.len() { speeds[index] } else { 0 }
}

pure visible(code: Int) -> Str {
  return "<undef>" when code == 0

  var rest = code
  var prefix = ""

  if rest >= 128 {
    prefix = "M-"
    rest -= 128
  }

  return f"{prefix}^{char_text(rest + 64)}" when rest < 32
  return f"{prefix}^?" when rest == 127

  f"{prefix}{char_text(rest)}"
}

# Items printed on one line each time they fit: an item moves to a new line
# when it would pass the screen width. Every section ends its line.
pure wrap_items(items: List[Str], width: Int, min_extra: Int) -> Str {
  var out = ""
  var column = 0

  for item in items {
    # Plain `stty` prints the min/time item with its newline, which counts as
    # width; `-a` does not.
    let length = item.count_chars() + (if item.starts_with("min = ") { min_extra } else { 0 })

    if column > 0 {
      if column + 1 + length > width {
        out = f"{out}\n"
        column = 0
      } else {
        out = f"{out} "
        column += 1
      }
    }

    out = f"{out}{item}"
    column += length
  }

  if column > 0 { f"{out}\n" } else { out }
}

proc screen_width() [process, env] -> Int {
  if let Ok(size) = unix.window_size(1) {
    return size.cols when size.cols > 0
  }

  let named = env.get_or("COLUMNS", "") ?? ""
  let parsed = named.parse_int_decimal() ?? 0

  if parsed > 0 and parsed <= 2147483647 { parsed } else { 80 }
}

pure word_of(attrs: UnixTtyAttrs, field: Str) -> Int {
  return attrs.iflag when field == "iflag"
  return attrs.oflag when field == "oflag"
  return attrs.cflag when field == "cflag"

  attrs.lflag
}

pure with_word(attrs: UnixTtyAttrs, field: Str, word: Int) -> UnixTtyAttrs {
  return {...attrs, iflag: word} when field == "iflag"
  return {...attrs, oflag: word} when field == "oflag"
  return {...attrs, cflag: word} when field == "cflag"

  {...attrs, lflag: word}
}

pure flag_named(table: UnixTtyTable, name: Str) -> UnixTtyFlag? {
  for flag in table.flags {
    return flag when flag.name == name
  }

  null
}

pure char_named(table: UnixTtyTable, name: Str) -> UnixTtyChar? {
  for char in table.chars {
    return char when char.name == name
  }

  null
}

# `csN`, `nlN` and the like carry a value in a masked field and cannot be negated.
pure is_grouped(name: Str) -> Bool {
  rx"^(cs|nl|cr|tab|bs|vt|ff)[0-9]$".matches(name)
}

pure flag_active(table: UnixTtyTable, attrs: UnixTtyAttrs, name: Str) -> Bool {
  guard let flag = flag_named(table, name) else {
    return false
  }

  word_of(attrs, flag.field).bit_and(flag.mask) == flag.value
}

pure set_flag(table: UnixTtyTable, attrs: UnixTtyAttrs, name: Str, on: Bool) -> UnixTtyAttrs {
  guard let flag = flag_named(table, name) else {
    return attrs
  }

  let word = word_of(attrs, flag.field)
  let cleared = word.clear_bits(flag.mask)

  with_word(attrs, flag.field, if on { cleared.bit_or(flag.value) } else { cleared })
}

pure set_char(table: UnixTtyTable, attrs: UnixTtyAttrs, name: Str, value: Int) -> UnixTtyAttrs {
  guard let char = char_named(table, name) else {
    return attrs
  }

  var chars = attrs.control_chars
  chars[char.index] = value
  {...attrs, control_chars: chars}
}

pure set_flags(table: UnixTtyTable, attrs: UnixTtyAttrs, names: List[Str], on: Bool) -> UnixTtyAttrs {
  var next = attrs

  for name in names {
    next = set_flag(table, next, name, on)
  }

  next
}

pure sane_char(table: UnixTtyTable, name: Str) -> Int {
  (char_named(table, name) ?? {index: 0, name: name, sane: 0}).sane
}

# A combination setting, as `set_mode` in GNU stty applies it.
pure apply_combo(table: UnixTtyTable, attrs: UnixTtyAttrs, name: Str, reversed: Bool) -> UnixTtyAttrs {
  if name == "evenp" or name == "parity" {
    if reversed {
      return set_flag(table, set_flag(table, attrs, "parenb", false), "cs8", true)
    }

    return set_flag(table, set_flag(table, set_flag(table, attrs, "parodd", false), "parenb", true), "cs7", true)
  }

  if name == "oddp" {
    if reversed {
      return set_flag(table, set_flag(table, attrs, "parenb", false), "cs8", true)
    }

    return set_flag(table, set_flag(table, set_flag(table, attrs, "parodd", true), "parenb", true), "cs7", true)
  }

  if name == "nl" {
    if reversed {
      return set_flags(table, set_flags(table, attrs, ["icrnl", "onlcr"], true), ["inlcr", "igncr", "ocrnl", "onlret"], false)
    }

    return set_flags(table, attrs, ["icrnl", "onlcr"], false)
  }

  if name == "ek" {
    return set_char(table, set_char(table, attrs, "erase", sane_char(table, "erase")), "kill", sane_char(table, "kill"))
  }

  return set_flag(table, attrs, "icanon", reversed) when name == "cbreak"

  if name == "pass8" {
    if reversed {
      return set_flag(table, set_flag(table, set_flag(table, attrs, "parenb", true), "istrip", true), "cs7", true)
    }

    return set_flag(table, set_flag(table, set_flag(table, attrs, "parenb", false), "istrip", false), "cs8", true)
  }

  if name == "litout" {
    if reversed {
      return set_flags(table, set_flag(table, attrs, "cs7", true), ["parenb", "istrip", "opost"], true)
    }

    return set_flags(table, set_flag(table, attrs, "cs8", true), ["parenb", "istrip", "opost"], false)
  }

  # GNU applies `decctlq` as `-ixany` (only the start character restarts
  # output) although its help says it is the same as `ixany`.
  return set_flag(table, attrs, "ixany", reversed) when name == "decctlq"

  if name == "lcase" or name == "LCASE" {
    return set_flags(table, attrs, ["xcase", "iuclc", "olcuc"], ! reversed)
  }

  return set_flags(table, attrs, ["echoe", "echoctl", "echoke"], true) when name == "crt"

  if name == "dec" {
    let chars = set_char(table, set_char(table, set_char(table, attrs, "intr", 3), "erase", 127), "kill", 21)
    return set_flag(table, set_flags(table, chars, ["echoe", "echoctl", "echoke"], true), "ixany", false)
  }

  if name == "tabs" {
    return set_flag(table, attrs, if reversed { "tab3" } else { "tab0" }, true)
  }

  attrs
}

# `raw` and `-cooked` are raw mode; `cooked` and `-raw` are cooked mode.
proc apply_raw(attrs: UnixTtyAttrs, name: Str, reversed: Bool) [process, error] -> UnixTtyAttrs {
  let cooked = (name == "cooked" and ! reversed) or (name == "raw" and reversed)

  unix.tty_mode(attrs, if cooked { "cooked" } else { "raw" })?
}

proc apply_sane(attrs: UnixTtyAttrs) [process, error] -> UnixTtyAttrs {
  unix.tty_mode(attrs, "sane")?
}

pure apply_saved(attrs: UnixTtyAttrs, state: List[Int], speeds: List[Int]) -> UnixTtyAttrs {
  var chars = attrs.control_chars

  for index in range(chars.len()) {
    chars[index] = state[4 + index]
  }

  let cflag = state[2]
  let output = speed_of_code(cflag.bit_and(CBAUD_MASK), speeds)
  let input_code = cflag.bit_and(CIBAUD_MASK) / CIBAUD_SHIFT
  let input = if input_code == 0 { output } else { speed_of_code(input_code, speeds) }

  {...attrs, iflag: state[0], oflag: state[1], cflag: cflag, lflag: state[3], control_chars: chars, ispeed: input, ospeed: output}
}

# A saved state: 4 + NCCS colon-separated hexadecimal fields (`sscanf` `%lx`:
# blanks and `0x` allowed before the digits), the control characters no
# larger than 255. As with GNU, text after the digits of the last field is
# ignored. Returns the values or null.
pure parse_saved(text: Str) -> List[Int]? {
  let parts = text.split(":")

  return null when parts.len() != 4 + NCCS

  let values: List[Int] = collect {
    for index in range(parts.len()) {
      let last = index == parts.len() - 1
      let captured = rx"^[ \t\n\r\f\v]*(?:0[xX])?([0-9a-fA-F]+)(.*)$".captures(parts[index])

      return null when captured.is_empty()
      return null when ! last and captured[2] != ""

      let digits = captured[1]
      var value = 0

      for position in range(digits.byte_len()) {
        value = value * 16 + hex_digit(digits.byte_at(position) ?? 0)

        return null when value > 4294967295
      }

      return null when index >= 4 and value > 255

      yield value
    }
  }

  values
}

# Reduce the arguments to option state and the settings GNU's loop leaves for
# apply_settings: an element is an option only when every letter of it is one
# of `a`, `g` or `F` (with its device) or it names a long option; anything
# else, `-echo` or `-ax`, is a setting, though the options inside it still count.
proc parse_arguments(argv: List[Str]) [process, env] -> Parsed {
  var all = false
  var save = false
  var file: Str? = null
  var help = false
  var version = false
  var marked = false
  var index = 0
  var finished = false

  let settings: List[Str] = collect {
    while index < argv.len() {
      let arg = argv[index]
      index += 1

      if finished {
        yield arg
        continue
      }

      if arg == "--" {
        finished = true
        continue
      }

      if arg.starts_with("--") {
        let equals = arg.find("=")
        let name = if equals == null { arg.byte_slice(2) } else { arg.byte_slice(2, length: equals - 2) }
        let longs = ["all", "save", "file", "help", "version"]
        let matches = [long for long in longs if long.starts_with(name)]
        let chosen = if name in longs { name } else if matches.len() == 1 { matches[0] } else { "" }

        if chosen == "all" and equals == null {
          all = true
        } else if chosen == "save" and equals == null {
          save = true
        } else if chosen == "help" and equals == null {
          help = true
          break
        } else if chosen == "version" and equals == null {
          version = true
          break
        } else if chosen == "file" and (equals != null or index < argv.len()) {
          if equals != null {
            file = arg.byte_slice(equals + 1)
          } else {
            file = argv[index]
            index += 1
          }
        } else {
          yield arg
        }

        continue
      }

      if arg.starts_with("-") and arg != "-" {
        let letters = arg.byte_slice(1)
        var position = 0
        var unknown = false
        var consumed_next = false
        var recognized_all = false
        var recognized_save = false
        var device: Str? = null

        while position < letters.byte_len() {
          let letter = letters.byte_slice(position, length: 1)
          position += 1

          if letter == "a" {
            recognized_all = true
          } else if letter == "g" {
            recognized_save = true
          } else if letter == "F" {
            let attached = letters.byte_slice(position)

            if attached != "" {
              device = attached
            } else if index < argv.len() {
              device = argv[index]
              consumed_next = true
            } else {
              unknown = true
            }

            position = letters.byte_len()
          } else {
            unknown = true
            break
          }
        }

        all = all or recognized_all
        save = save or recognized_save

        if let named = device {
          file = named
        }

        if consumed_next {
          index += 1
        }

        if unknown {
          marked = true
          yield arg
        }

        continue
      }

      yield arg
    }
  }

  {all: all, save: save, file: file, help: help, version: version, settings: settings, marked: marked}
}

proc needs_argument(arg: Str, settings: List[Str], at: Int) [process, env] {
  if at + 1 >= settings.len() {
    gnu.usage_error(f"missing argument to {gnu.quote_value(arg)}")
  }
}

proc invalid_argument(arg: Str) [process, env] {
  gnu.usage_error(f"invalid argument {gnu.quote_value(arg)}")
}

# Check every setting before the terminal is touched, as GNU's first pass
# does, and return the typed list the apply pass walks.
proc check_settings(table: UnixTtyTable, texts: List[Str]) [process, env, error] -> List[Setting] {
  var at = 0

  let checked: List[Setting] = collect {
    while at < texts.len() {
      let arg = texts[at]
      let reversed = arg.starts_with("-")
      let word = if reversed { arg.byte_slice(1) } else { arg }
      let name = alias_of(word)

      if reversed and (is_grouped(word) or word in COMBOS_PLAIN or word == "sane") {
        invalid_argument(arg)
      }

      if flag_named(table, name) != null and (! reversed or ! is_grouped(name)) {
        yield {kind: "flag", name: name, reversed: reversed, text: "", number: 0}
      } else if flag_named(table, name) != null {
        invalid_argument(arg)
      } else if word == "drain" {
        yield {kind: "drain", name: "drain", reversed: reversed, text: "", number: 0}
      } else if ! reversed and char_named(table, word) != null and word != "min" and word != "time" {
        needs_argument(arg, texts, at)
        at += 1
        yield {kind: "char", name: word, reversed: false, text: texts[at], number: char_value(texts[at])}
      } else if ! reversed and (word == "min" or word == "time") {
        needs_argument(arg, texts, at)
        at += 1
        yield {kind: "number", name: word, reversed: false, text: texts[at], number: integer_arg(texts[at], 255)}
      } else if ! reversed and (word == "ispeed" or word == "ospeed") {
        needs_argument(arg, texts, at)
        at += 1

        let speed = baud_value(texts[at], table.speeds)

        if speed < 0 {
          gnu.usage_error(f"invalid {word} {gnu.quote_value(texts[at])}")
        }

        yield {kind: word, name: word, reversed: false, text: texts[at], number: speed}
      } else if ! reversed and (word == "rows" or word == "cols" or word == "columns") {
        needs_argument(arg, texts, at)
        at += 1

        let size = integer_arg(texts[at], 4294967295)

        yield {
          kind: if word == "rows" { "rows" } else { "cols" },
          name: word,
          reversed: false,
          text: texts[at],
          number: size % 65536,
        }
      } else if ! reversed and word == "line" {
        needs_argument(arg, texts, at)
        at += 1
        yield {kind: "line", name: word, reversed: false, text: texts[at], number: integer_arg(texts[at], 255)}
      } else if ! reversed and (word == "size" or word == "speed") {
        yield {kind: word, name: word, reversed: false, text: "", number: 0}
      } else if word in COMBOS_PLAIN or word in COMBOS_NEGATABLE {
        yield {kind: "combo", name: word, reversed: reversed, text: "", number: 0}
      } else if ! reversed and parse_saved(arg) != null {
        yield {kind: "saved", name: "saved", reversed: false, text: arg, number: 0}
      } else if baud_value(arg, table.speeds) >= 0 and ! reversed {
        yield {kind: "both", name: "speed", reversed: false, text: arg, number: baud_value(arg, table.speeds)}
      } else {
        invalid_argument(arg)
      }

      at += 1
    }
  }

  checked
}

proc device_error(device: Device, failure: Error) [process, env] {
  gnu.error(f"{gnu.quote_maybe(device.name)}: {gnu.strerror(failure)}")
  exit 1
}

proc open_device(file: Str?) [process, env] -> Device {
  guard let named = file else {
    return {fd: 0, name: "standard input", opened: false}
  }

  match unix.open_fd(fp"{named}", nonblock: true) {
    Ok(fd) => {
      {fd: fd, name: named, opened: true}
    }
    Err(failure) => {
      gnu.error(f"{gnu.quote_maybe(named)}: {gnu.strerror(failure)}")
      exit 1
    }
  }
}

proc release(device: Device) [process, error] {
  if device.opened {
    unix.close_fd(device.fd)
  }
}

proc read_attrs(device: Device) [process, env, error] -> UnixTtyAttrs {
  match unix.tty_attrs(device.fd) {
    Ok(attrs) => attrs
    Err(failure) => {
      device_error(device, failure)
      exit 1
    }
  }
}

pure speed_item(attrs: UnixTtyAttrs) -> Str {
  if attrs.ispeed == 0 or attrs.ispeed == attrs.ospeed {
    return f"speed {attrs.ospeed} baud;"
  }

  f"ispeed {attrs.ispeed} baud; ospeed {attrs.ospeed} baud;"
}

# The words of one group of flags. With `everything` each flag shows as `name`
# or `-name` and a grouped flag shows only the member that is selected;
# otherwise only the deviations from `stty sane` show.
pure flag_items(table: UnixTtyTable, attrs: UnixTtyAttrs, names: List[Str], everything: Bool) -> List[Str] {
  let items: List[Str] = collect {
    for name in names {
      guard let flag = flag_named(table, name) else {
        continue
      }

      let active = word_of(attrs, flag.field).bit_and(flag.mask) == flag.value

      if is_grouped(name) {
        yield name when active and (everything or ! flag.sane)
      } else if active {
        yield name when everything or name in SANE_CLEARED
      } else if everything or flag.sane {
        yield f"-{name}"
      }
    }
  }

  items
}

proc display_settings(table: UnixTtyTable, attrs: UnixTtyAttrs, device: Device, everything: Bool) [process, env, io] {
  let width = screen_width()
  var head = [speed_item(attrs)]

  if everything {
    if let Ok(size) = unix.window_size(device.fd) {
      head += [f"rows {size.rows}; columns {size.cols};"]
    }
  }

  head += [f"line = {attrs.line};"]

  var chars: List[Str] = []

  for name in CHAR_NAMES {
    let char = char_named(table, name) ?? {index: 0, name: name, sane: 0}
    let value = attrs.control_chars[char.index]

    if everything or value != char.sane {
      chars += [f"{name} = {visible(value)};"]
    }
  }

  let min_index = (char_named(table, "min") ?? {index: 6, name: "min", sane: 1}).index
  let time_index = (char_named(table, "time") ?? {index: 5, name: "time", sane: 0}).index

  if everything or ! flag_active(table, attrs, "icanon") {
    chars += [f"min = {attrs.control_chars[min_index]}; time = {attrs.control_chars[time_index]};"]
  }

  var text = wrap_items(head, width, 0) + wrap_items(chars, width, if everything { 0 } else { 1 })

  for names in [CONTROL_FLAGS, INPUT_FLAGS, OUTPUT_FLAGS, LOCAL_FLAGS] {
    text = text + wrap_items(flag_items(table, attrs, names, everything), width, 0)
  }

  gnu.write_text(text)
}

# The saved form of the state, with the input-speed bits dropped when both
# speeds agree.
pure save_text(attrs: UnixTtyAttrs) -> Str {
  let cflag = if attrs.ispeed == attrs.ospeed or attrs.ispeed == 0 { attrs.cflag.clear_bits(CIBAUD_MASK) } else { attrs.cflag }
  var fields = [hex(attrs.iflag), hex(attrs.oflag), hex(cflag), hex(attrs.lflag)]

  for index in range(NCCS) {
    fields += [hex(if index < attrs.control_chars.len() { attrs.control_chars[index] } else { 0 })]
  }

  fields |> join(":")
}

# Write the state, then read it back: like GNU, a terminal that quietly kept
# some other state (a pty refuses parity and sizes other than 8 bits) is an
# error. Equal speeds are written as one so the input-speed bits stay clear.
proc apply_attrs(device: Device, wanted: UnixTtyAttrs, moment: Str) [process, env, error] {
  let input = if wanted.ispeed == wanted.ospeed { 0 } else { wanted.ispeed }

  if let Err(failure) = unix.set_tty_attrs({...wanted, ispeed: input}, device.fd, moment) {
    device_error(device, failure)
  }

  let actual = read_attrs(device)
  let expected_input = if input == 0 { wanted.ospeed } else { input }
  let same_speeds = actual.ispeed == expected_input and actual.ospeed == wanted.ospeed
  let speed_bits = CBAUD_MASK + CIBAUD_MASK
  let same_state = actual.iflag == wanted.iflag and actual.oflag == wanted.oflag and actual.lflag == wanted.lflag and actual.line == wanted.line and actual.cflag.clear_bits(speed_bits) == wanted.cflag.clear_bits(speed_bits) and actual.control_chars == wanted.control_chars

  if ! (same_state and same_speeds) {
    gnu.error(f"{gnu.quote_maybe(device.name)}: unable to perform all requested operations")
    exit 1
  }
}

# `drain` and `-drain` choose when the state is written; alone they leave a
# plain `stty`, which prints.
pure has_modes(settings: List[Setting]) -> Bool {
  for setting in settings {
    return true when setting.kind != "drain"
  }

  false
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let parsed = parse_arguments(argv)

  if parsed.help {
    gnu.help(USAGE)
    return
  }

  if parsed.version {
    gnu.version("stty")
    return
  }

  if parsed.all and parsed.save {
    gnu.error("the options for verbose and stty-readable output styles are\nmutually exclusive")
    exit 1
  }

  if (parsed.all or parsed.save) and (! parsed.settings.is_empty() or parsed.marked) {
    gnu.error("when specifying an output style, modes may not be set")
    exit 1
  }

  let table = unix.tty_table()
  let settings = check_settings(table, parsed.settings)
  let device = open_device(parsed.file)

  defer release(device)

  var attrs = read_attrs(device)

  if parsed.save {
    gnu.write_text(f"{save_text(attrs)}\n")
  } else if parsed.all or ! has_modes(settings) {
    display_settings(table, attrs, device, parsed.all)
  } else {
    var changed = false
    var unkept = false
    var speed_set = false
    var input_speed = -1
    var output_speed = -1
    var moment = "drain"

    for setting in settings {
      if setting.kind == "flag" {
        attrs = set_flag(table, attrs, setting.name, ! setting.reversed)
        changed = true
      } else if setting.kind == "char" or setting.kind == "number" {
        attrs = set_char(table, attrs, setting.name, setting.number)
        changed = true
      } else if setting.kind == "combo" {
        if setting.name == "sane" {
          attrs = apply_sane(attrs)
        } else if setting.name == "raw" or setting.name == "cooked" {
          attrs = apply_raw(attrs, setting.name, setting.reversed)
        } else {
          attrs = apply_combo(table, attrs, setting.name, setting.reversed)
        }

        changed = true
      } else if setting.kind == "ispeed" {
        input_speed = setting.number
        changed = true
        speed_set = true
      } else if setting.kind == "ospeed" {
        output_speed = setting.number
        changed = true
        speed_set = true
      } else if setting.kind == "both" {
        input_speed = setting.number
        output_speed = setting.number
        changed = true
        speed_set = true
      } else if setting.kind == "saved" {
        let state = parse_saved(setting.text) ?? []
        attrs = apply_saved(attrs, state, table.speeds)
        changed = true

        # The kernel keeps only its own control characters, so a saved
        # state that sets later ones cannot be fully applied.
        for slot in range(KERNEL_CHARS, NCCS) {
          if state[4 + slot] != 0 {
            unkept = true
          }
        }
      } else if setting.kind == "line" {
        attrs = {...attrs, line: setting.number}
        changed = true
      } else if setting.kind == "drain" {
        moment = if setting.reversed { "now" } else { "drain" }
      } else if setting.kind == "rows" or setting.kind == "cols" {
        match unix.window_size(device.fd) {
          Ok(size) => {
            let rows = if setting.kind == "rows" { setting.number } else { size.rows }
            let cols = if setting.kind == "cols" { setting.number } else { size.cols }

            if let Err(failure) = unix.set_window_size(rows, cols, size.xpixel, size.ypixel, device.fd) {
              device_error(device, failure)
            }
          }
          Err(failure) => device_error(device, failure)
        }
      } else if setting.kind == "size" {
        match unix.window_size(device.fd) {
          Ok(size) => gnu.write_text(f"{size.rows} {size.cols}\n")
          Err(failure) => device_error(device, failure)
        }
      } else if setting.kind == "speed" {
        gnu.write_text(f"{attrs.ospeed}\n")
      }
    }

    # Linux keeps one speed: setting either direction sets both, and two
    # different requested speeds cannot be honored.
    if input_speed >= 0 and output_speed >= 0 and input_speed != output_speed {
      gnu.error(f"asymmetric input ({input_speed}), output ({output_speed}) speeds not supported")
      exit 1
    }

    if speed_set {
      let speed = if input_speed >= 0 { input_speed } else { output_speed }
      attrs = {...attrs, ispeed: speed, ospeed: speed}
    }

    if changed {
      apply_attrs(device, attrs, moment)
    }

    if unkept {
      gnu.error(f"{gnu.quote_maybe(device.name)}: unable to perform all requested operations")
      exit 1
    }
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}

