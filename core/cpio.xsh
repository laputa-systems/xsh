#!/bin/xsh
use lib.gnu

# A fatal failure ends the run at once with status 2. Failures GNU cpio counts as
# errors but survives (a file that cannot be created, a missing input name) only
# set `Run.failed`, which decides the final status after the block count.
error CpioError = Fatal(message: Str)

const USAGE = """Usage: cpio [OPTION...] [destination-directory]
GNU `cpio' copies files to and from archives

Examples:
  # Copy files named in name-list to the archive
  cpio -o < name-list [> archive]
  # Extract files from the archive
  cpio -i [< archive]
  # Copy files named in name-list to destination-directory
  cpio -p destination-directory < name-list

 Main operation mode:
  -i, --extract              Extract files from an archive (run in copy-in
                             mode)
  -o, --create               Create the archive (run in copy-out mode)
  -p, --pass-through         Run in copy-pass mode
  -t, --list                 Print a table of contents of the input

 Operation modifiers valid in any mode:

      --block-size=BLOCK-SIZE   Set the I/O block size to BLOCK-SIZE * 512
                             bytes
  -B                         Set the I/O block size to 5120 bytes
  -c                         Use the old portable (ASCII) archive format
  -C, --io-size=NUMBER       Set the I/O block size to the given NUMBER of
                             bytes
  -D, --directory=DIR        Change to directory DIR
      --force-local          Archive file is local, even if its name contains
                             colons
  -H, --format=FORMAT        Use given archive FORMAT
      --quiet                Do not print the number of blocks copied
  -R, --owner=[USER][:.][GROUP]   Set the ownership of all files created to the
                             specified USER and/or GROUP
  -v, --verbose              Verbosely list the files processed
  -V, --dot                  Print a "." for each file processed
  -W, --warning=FLAG         Control warning display. Currently FLAG is one of
                             'none', 'truncate', 'all'. Multiple options
                             accumulate.

 Operation modifiers valid in copy-in and copy-out modes

  -F, --file=[[USER@]HOST:]FILE-NAME
                             Use this FILE-NAME instead of standard input or
                             output. Optional USER and HOST specify the user
                             and host names in case of a remote archive
  -M, --message=STRING       Print STRING when the end of a volume of the
                             backup media is reached
      --rsh-command=COMMAND  Use COMMAND instead of rsh

 Operation modifiers valid only in copy-in mode:

  -b, --swap                 Swap both halfwords of words and bytes of
                             halfwords in the data. Equivalent to -sS
  -f, --nonmatching          Only copy files that do not match any of the given
                             patterns
  -I [[USER@]HOST:]FILE-NAME Archive filename to use instead of standard input.
                             Optional USER and HOST specify the user and host
                             names in case of a remote archive
  -n, --numeric-uid-gid      In the verbose table of contents listing, show
                             numeric UID and GID
  -r, --rename               Interactively rename files
  -s, --swap-bytes           Swap the bytes of each halfword in the files
  -S, --swap-halfwords       Swap the halfwords of each word (4 bytes) in the
                             files
      --to-stdout            Extract files to standard output

  -E, --pattern-file=FILE    Read additional patterns specifying filenames to
                             extract or list from FILE
      --only-verify-crc      When reading a CRC format archive, only verify the
                             CRC's of each file in the archive, don't actually
                             extract the files

 Operation modifiers valid only in copy-out mode:

  -A, --append               Append to an existing archive.
      --device-independent, --reproducible
                             Create device-independent (reproducible) archives
      --ignore-devno         Don't store device numbers
      --ignore-dirnlink      ignore number of links of a directory; always
                             assume 2
  -O [[USER@]HOST:]FILE-NAME Archive filename to use instead of standard
                             output. Optional USER and HOST specify the user
                             and host names in case of a remote archive
      --renumber-inodes      Renumber inodes

 Operation modifiers valid only in copy-pass mode:

  -l, --link                 Link files instead of copying them, when
                             possible

 Operation modifiers valid in copy-in and copy-out modes:

      --absolute-filenames   Do not strip file system prefix components from
                             the file names
      --no-absolute-filenames   Create all files relative to the current
                             directory

 Operation modifiers valid in copy-out and copy-pass modes:

  -0, --null                 Filenames in the list are delimited by null
                             characters instead of newlines
  -a, --reset-access-time    Reset the access times of files after reading
                             them
  -L, --dereference          Dereference  symbolic  links  (copy  the files
                             that they point to instead of copying the links).

 Operation modifiers valid in copy-in and copy-pass modes:

  -d, --make-directories     Create leading directories where needed
  -m, --preserve-modification-time
                             Retain previous file modification times when
                             creating files
      --no-preserve-owner    Do not change the ownership of the files
      --sparse               Write files with large blocks of zeros as sparse
                             files
  -u, --unconditional        Replace all files unconditionally

  -?, --help                 give this help list
      --usage                give a short usage message
      --version              print program version

Mandatory or optional arguments to long options are also mandatory or optional
for any corresponding short options.
"""

const SHORT_USAGE = """Usage: cpio [-ioptBcvVbfnrsSAl0aLdmu?] [-C NUMBER] [-D DIR] [-H FORMAT]
            [-R [USER][:.][GROUP]] [-W FLAG] [-F [[USER@]HOST:]FILE-NAME]
            [-M STRING] [-I [[USER@]HOST:]FILE-NAME] [-E FILE]
            [-O [[USER@]HOST:]FILE-NAME] [--extract] [--create]
            [--pass-through] [--list] [--block-size=BLOCK-SIZE]
            [--io-size=NUMBER] [--directory=DIR] [--force-local]
            [--format=FORMAT] [--quiet] [--owner=[USER][:.][GROUP]] [--verbose]
            [--dot] [--warning=FLAG] [--file=[[USER@]HOST:]FILE-NAME]
            [--message=STRING] [--rsh-command=COMMAND] [--swap] [--nonmatching]
            [--numeric-uid-gid] [--rename] [--swap-bytes] [--swap-halfwords]
            [--to-stdout] [--pattern-file=FILE] [--only-verify-crc] [--append]
            [--device-independent] [--reproducible] [--ignore-devno]
            [--ignore-dirnlink] [--renumber-inodes] [--link]
            [--absolute-filenames] [--no-absolute-filenames] [--null]
            [--reset-access-time] [--dereference] [--make-directories]
            [--preserve-modification-time] [--no-preserve-owner] [--sparse]
            [--unconditional] [--help] [--usage] [--version]
            [destination-directory]
"""

const TRAILER = b"TRAILER!!!"
const ZERO_PAD = b"\0\0\0\0"
const NEWC_HEADER = 110
const ODC_HEADER = 76
const BIN_HEADER = 26
const TYPE_MASK = 0o170000
const TYPE_FILE = 0o100000
const TYPE_DIR = 0o040000
const TYPE_LINK = 0o120000
const TYPE_CHAR = 0o020000
const TYPE_BLOCK = 0o060000
const TYPE_FIFO = 0o010000
const TYPE_SOCKET = 0o140000
const HEX_DIGITS = "0123456789ABCDEF"
const CHUNK = 1048576
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
const HALF_YEAR_SECONDS = 15778476

# Two uppercase hex digits for every byte value, so a 32-bit header field costs
# four lookups instead of eight digit extractions.
let HEX_PAIRS = [f"{HEX_DIGITS.byte_slice(value / 16, length: 1)}{HEX_DIGITS.byte_slice(value % 16, length: 1)}" for value in range(256)]

type Options = {
  mode: Str,
  table: Bool,
  verbose: Bool,
  dot: Bool,
  quiet: Bool,
  format: Str,
  block_size: Int,
  directory: Str?,
  force_local: Bool,
  owner: Int?,
  group: Int?,
  warn_truncate: Bool,
  warn_interdir: Bool,
  archive: Str?,
  input_archive: Str?,
  output_archive: Str?,
  swap_bytes: Bool,
  swap_halfwords: Bool,
  nonmatching: Bool,
  numeric: Bool,
  pattern_file: Str?,
  only_verify_crc: Bool,
  rename: Bool,
  rename_batch: Str?,
  to_stdout: Bool,
  append: Bool,
  renumber: Bool,
  ignore_devno: Bool,
  ignore_dirnlink: Bool,
  link: Bool,
  no_abs: Bool,
  null: Bool,
  reset_time: Bool,
  dereference: Bool,
  make_dirs: Bool,
  preserve_mtime: Bool,
  no_preserve_owner: Bool,
  sparse: Bool,
  unconditional: Bool,
  help: Bool,
  usage: Bool,
  version: Bool,
  operands: List[Str],
}

# The fields every format shares. `name` is raw bytes so archives with names
# that are not UTF-8 survive a round trip.
type Header = {
  ino: Int,
  mode: Int,
  uid: Int,
  gid: Int,
  nlink: Int,
  mtime: Int,
  size: Int,
  dev_major: Int,
  dev_minor: Int,
  rdev_major: Int,
  rdev_minor: Int,
  checksum: Int,
  name: Bytes,
}

# One header read from the archive: where its data starts, the format in force
# (detected from the first header), and whether the name was usable.
type Parsed = {header: Header, at: Int, data: Int, format: Str, usable: Bool, reversed: Bool}

# A link whose data travels with another link of the same inode.
type Deferred = {header: Header, source: Bytes, atime_ns: Int, mtime_ns: Int}

# A directory whose mode or time is applied after its contents exist.
type DirFixup = {path: Path, name: Bytes, mode: Int, uid: Int, gid: Int, mtime: Int}

# With --no-absolute-filenames symbolic links are first created as empty
# placeholders and swapped for the real link at the end, so a later member can
# never be written through a link the archive itself planted.
type Placeholder = {path: Path, name: Bytes, target: Bytes, mode: Int, uid: Int, gid: Int, mtime: Int, dev: Int, ino: Int}

# Mutable run state, threaded through the per-entry procedures by value.
type Run = {
  failed: Bool,
  prefixes: List[Bytes],
  inodes: Map[Path],
  deferred: List[Header],
  placeholders: List[Placeholder],
  fixups: List[DirFixup],
  copied: Int,
  warned_reverse: Bool,
}

type Named = {name: Bytes, st: Run}
type Created = {ok: Bool, st: Run}
type Linked = {linked: Bool, reason: Str, st: Run}
type Safer = {name: Bytes, prefix: Bytes, substituted: Bool}
type Bracket = {valid: Bool, matched: Bool, next: Int}
type UserSpec = {uid: Int?, gid: Int?}

# The display form of a raw name for messages: invalid UTF-8 is replaced.
proc shown(name: Bytes) [process] -> Str {
  match Path.parse_bytes(name) {
    Ok(parsed) => parsed.display()
    Err(_) => name.utf8() ?? "?"
  }
}

# A failed write ends the run with status 2 after GNU cpio's wording, except a
# closed pipe, which ends it as SIGPIPE would.
proc write_out(data: Bytes) [io, process, env] -> Unit {
  var failed: Error? = null
  match io.write_stdout_bytes(data) {
    Ok(_) => {}
    Err(failure) => { failed = failure }
  }
  if failed == null {
    match io.flush_stdout() {
      Ok(_) => {}
      Err(failure) => { failed = failure }
    }
  }
  if let failure = failed {
    if gnu.errno(failure) == 32 { exit 141 }
    gnu.error(f"write error: {gnu.strerror(failure)}")
    exit 2
  }
}

proc die(message: Str) [process, env] -> Unit {
  gnu.error(message)
  exit 2
}

# argp reports a bad command line with a hint and its own status.
proc usage_failure(message: Str, status: Int) [process, env] -> Unit {
  eprint f"{gnu.prog()}: {message}"
  eprint f"Try '{gnu.prog()} --help' or '{gnu.prog()} --usage' for more information."
  exit status
}

# ---------------------------------------------------------------------------
# Numbers and header fields
# ---------------------------------------------------------------------------

pure hex_digit(byte: Int) -> Int {
  if byte >= 48 and byte <= 57 {
    byte - 48
  } else if byte >= 65 and byte <= 70 {
    byte - 55
  } else if byte >= 97 and byte <= 102 {
    byte - 87
  } else {
    -1
  }
}

# Reads a header number the way GNU cpio does: leading spaces and a NUL end are
# honored and a non-digit stops the number after a warning, keeping the value
# parsed so far. `bits` is 4 for hex and 3 for octal fields.
proc ascii_number(data: Bytes, at: Int, digits: Int, bits: Int) [process, env] -> Int {
  var position = at
  let end = at + digits
  while position < end and data.byte_at(position) == 32 { position += 1 }
  if position == end or (data.byte_at(position) ?? 0) == 0 { return 0 }
  var value = 0
  while true {
    let digit = hex_digit(data.byte_at(position) ?? 0)
    if digit < 0 {
      let field = data.slice(at, digits).utf8() ?? "?"
      gnu.error(f"Malformed number {field}")
      break
    }
    value += digit
    position += 1
    if position == end or (data.byte_at(position) ?? 0) == 0 { break }
    value = value * (if bits == 4 { 16 } else { 8 })
  }
  value
}

# The common case is a field of well-formed digits, parsed in one step.
proc hex_field(data: Bytes, at: Int) [process, env] -> Int {
  let text = data.slice(at, 8).utf8() ?? ""
  if rx"^[0-9A-Fa-f]{8}$".matches(text) {
    f"0x{text}".parse_int() ?? 0
  } else {
    ascii_number(data, at, 8, 4)
  }
}

proc octal_field(data: Bytes, at: Int, digits: Int) [process, env] -> Int {
  let text = data.slice(at, digits).utf8() ?? ""
  if rx"^[0-7]+$".matches(text) {
    f"0o{text}".parse_int() ?? 0
  } else {
    ascii_number(data, at, digits, 3)
  }
}

pure word_at(data: Bytes, at: Int, big_endian: Bool) -> Int {
  let low = data.byte_at(at) ?? 0
  let high = data.byte_at(at + 1) ?? 0
  if big_endian { low * 256 + high } else { high * 256 + low }
}

pure hex_text(value: Int) -> Str {
  let v = value.bit_and(4294967295)
  f"{HEX_PAIRS[v / 16777216 % 256]}{HEX_PAIRS[v / 65536 % 256]}{HEX_PAIRS[v / 256 % 256]}{HEX_PAIRS[v % 256]}"
}

# Octal text of exactly `digits` digits; higher digits that do not fit are cut,
# which is what GNU cpio does before it warns about truncation.
pure octal_text(value: Int, digits: Int) -> Str {
  var left = value
  var text = ""
  for _ in range(digits) {
    text = f"{"01234567".byte_slice(left % 8, length: 1)}{text}"
    left = left / 8
  }
  text
}

pure fits_octal(value: Int, digits: Int) -> Bool {
  var limit = 1
  for _ in range(digits) { limit = limit * 8 }
  value < limit
}

# Linux dev_t layout: the low 8 bits of the minor number, the low 12 bits of the
# major number, then the remaining minor and major bits above them.
pure device_major(device: Int) -> Int {
  device / 256 % 4096 + device / 4294967296 / 4096 * 4096
}

pure device_minor(device: Int) -> Int {
  device % 256 + device / 4096 % 4294967296 / 256 * 256
}

pure make_device(major: Int, minor: Int) -> Int {
  major % 4096 * 256 + minor % 256 + minor / 256 * 1048576 + major / 4096 * 4096 * 4294967296
}

pure le_word(value: Int) -> Bytes {
  match bytes.from_ints([value % 256, value / 256 % 256]) {
    Ok(word) => word
    Err(_) => b"\0\0"
  }
}

# The sum of all data bytes modulo 2^32, which is the crc format's checksum.
pure byte_sum(data: Bytes) -> Int {
  var total = 0
  for byte in data { total += byte }
  total % 4294967296
}

pure padding(format: Str, length: Int) -> Int {
  match format {
    "newc" | "crc" => (4 - length % 4) % 4
    "bin" | "hpbin" => length % 2
    _ => 0
  }
}

pure kind_bits(mode: Int) -> Int {
  mode.bit_and(TYPE_MASK)
}

# The format named by the first bytes of an archive, or "" when none matches.
pure detect_format(data: Bytes, at: Int) -> Str {
  let head = data.slice(at, 6)
  if head == b"070701" {
    "newc"
  } else if head == b"070707" {
    "odc"
  } else if head == b"070702" {
    "crc"
  } else if word_at(data, at, false) == 0o070707 or word_at(data, at, true) == 0o070707 {
    "bin"
  } else {
    ""
  }
}

pure magic_matches(data: Bytes, at: Int, format: Str) -> Bool {
  match format {
    "newc" => data.slice(at, 6) == b"070701"
    "crc" => data.slice(at, 6) == b"070702"
    "odc" | "hpodc" => data.slice(at, 6) == b"070707"
    _ => word_at(data, at, false) == 0o070707 or word_at(data, at, true) == 0o070707
  }
}

# Parses the header at `start`, skipping junk bytes before a recognizable magic.
# `known` is the format already in force, or "" for the first header. Short
# input is fatal, as in GNU cpio.
proc read_header(data: Bytes, start: Int, known: Str, warned_reverse: Bool) [process, env] -> Result[Parsed, CpioError] {
  var position = start
  var format = known
  var junk = 0
  while true {
    if data.len() - position < 6 {
      let message = if known == "" { "premature end of archive" } else { "premature end of file" }
      return Err(CpioError.Fatal(message: message))
    }
    if known == "" and data.len() - position >= 512 and data.slice(position + 257, 5) == b"ustar" and detect_format(data, position) == "" {
      return Err(CpioError.Fatal(message: "tar and ustar archives are not supported"))
    }
    if format == "" {
      format = detect_format(data, position)
      if format != "" { break }
    } else if magic_matches(data, position, format) {
      break
    }
    position += 1
    junk += 1
  }
  if junk > 0 {
    gnu.error(if junk == 1 { f"warning: skipped {junk} byte of junk" } else { f"warning: skipped {junk} bytes of junk" })
  }
  let width = match format {
    "newc" | "crc" => NEWC_HEADER
    "odc" | "hpodc" => ODC_HEADER
    _ => BIN_HEADER
  }
  if data.len() - position < width {
    return Err(CpioError.Fatal(message: "premature end of file"))
  }
  var header: Header = {ino: 0, mode: 0, uid: 0, gid: 0, nlink: 0, mtime: 0, size: 0, dev_major: 0, dev_minor: 0, rdev_major: 0, rdev_minor: 0, checksum: 0, name: b""}
  var name_size = 0
  var reversed = false
  if format == "newc" or format == "crc" {
    header = {
      ino: hex_field(data, position + 6),
      mode: hex_field(data, position + 14),
      uid: hex_field(data, position + 22),
      gid: hex_field(data, position + 30),
      nlink: hex_field(data, position + 38),
      mtime: hex_field(data, position + 46),
      size: hex_field(data, position + 54),
      dev_major: hex_field(data, position + 62),
      dev_minor: hex_field(data, position + 70),
      rdev_major: hex_field(data, position + 78),
      rdev_minor: hex_field(data, position + 86),
      checksum: hex_field(data, position + 102),
      name: b"",
    }
    name_size = hex_field(data, position + 94)
  } else if format == "odc" or format == "hpodc" {
    let device = octal_field(data, position + 6, 6)
    let rdev = octal_field(data, position + 42, 6)
    header = {
      ino: octal_field(data, position + 12, 6),
      mode: octal_field(data, position + 18, 6),
      uid: octal_field(data, position + 24, 6),
      gid: octal_field(data, position + 30, 6),
      nlink: octal_field(data, position + 36, 6),
      mtime: octal_field(data, position + 48, 11),
      size: octal_field(data, position + 65, 11),
      dev_major: device_major(device),
      dev_minor: device_minor(device),
      rdev_major: device_major(rdev),
      rdev_minor: device_minor(rdev),
      checksum: 0,
      name: b"",
    }
    name_size = octal_field(data, position + 59, 6)
  } else {
    reversed = word_at(data, position, true) == 0o070707 and word_at(data, position, false) != 0o070707
    let device = word_at(data, position + 2, reversed)
    let rdev = word_at(data, position + 14, reversed)
    header = {
      ino: word_at(data, position + 4, reversed),
      mode: word_at(data, position + 6, reversed),
      uid: word_at(data, position + 8, reversed),
      gid: word_at(data, position + 10, reversed),
      nlink: word_at(data, position + 12, reversed),
      mtime: word_at(data, position + 16, reversed) * 65536 + word_at(data, position + 18, reversed),
      size: word_at(data, position + 22, reversed) * 65536 + word_at(data, position + 24, reversed),
      dev_major: device_major(device),
      dev_minor: device_minor(device),
      rdev_major: device_major(rdev),
      rdev_minor: device_minor(rdev),
      checksum: 0,
      name: b"",
    }
    name_size = word_at(data, position + 20, reversed)
    if reversed and ! warned_reverse {
      gnu.error("warning: archive header has reverse byte-order")
    }
  }
  var usable = true
  var name = b""
  var cursor = position + width
  if name_size == 0 {
    gnu.error("malformed header: file name of zero length")
    usable = false
  } else {
    if data.len() - cursor < name_size {
      return Err(CpioError.Fatal(message: "premature end of file"))
    }
    let raw = data.slice(cursor, name_size)
    cursor += name_size
    if raw.byte_at(name_size - 1) != 0 {
      gnu.error("malformed header: file name is not nul-terminated")
      usable = false
      name_size = 0
    } else {
      name = raw.slice(0, name_size - 1)
    }
  }
  if format == "newc" or format == "crc" {
    cursor += padding(format, name_size + NEWC_HEADER)
  } else if format == "bin" or format == "hpbin" {
    cursor += name_size % 2
  }
  var size = header.size
  var rdev_major = header.rdev_major
  var rdev_minor = header.rdev_minor
  let kind = kind_bits(header.mode)
  if (format == "odc" or format == "hpodc" or format == "bin" or format == "hpbin") and size != 0 and rdev_major == 0 and rdev_minor == 1 {
    if kind == TYPE_CHAR or kind == TYPE_BLOCK or kind == TYPE_FIFO or kind == TYPE_SOCKET {
      rdev_major = device_major(size)
      rdev_minor = device_minor(size)
      size = 0
    }
  }
  Ok({
    header: {...header, name: name, size: size, rdev_major: rdev_major, rdev_minor: rdev_minor},
    at: position,
    data: cursor,
    format: format,
    usable: usable,
    reversed: reversed,
  })
}

# Everything a header encoder needs is in the header; the checksum field is only
# meaningful for the crc format.
proc encode_header(format: Str, header: Header, truncate_warnings: Bool) [process, env] -> Bytes? {
  let name = header.name
  let name_size = name.len() + 1
  let shown_name = shown(name)
  if format == "newc" or format == "crc" {
    let wide = [
      {label: "file size", value: header.size},
      {label: "device major number", value: header.dev_major},
      {label: "device minor number", value: header.dev_minor},
      {label: "rdev major", value: header.rdev_major},
      {label: "rdev minor", value: header.rdev_minor},
      {label: "name size", value: name_size},
    ]
    for field in wide {
      if field.value > 4294967295 {
        gnu.error(f"{shown_name}: value {field.label} {field.value} out of allowed range 0..4294967295")
        return null
      }
    }
    if truncate_warnings {
      let narrow = [
        {label: "inode number", value: header.ino},
        {label: "file mode", value: header.mode},
        {label: "uid", value: header.uid},
        {label: "gid", value: header.gid},
        {label: "number of links", value: header.nlink},
        {label: "modification time", value: header.mtime},
      ]
      for field in narrow {
        if field.value > 4294967295 { gnu.error(f"{shown_name}: truncating {field.label}") }
      }
    }
    let magic = if format == "crc" { "070702" } else { "070701" }
    let text = f"{magic}{hex_text(header.ino)}{hex_text(header.mode)}{hex_text(header.uid)}{hex_text(header.gid)}{hex_text(header.nlink)}{hex_text(header.mtime)}{hex_text(header.size)}{hex_text(header.dev_major)}{hex_text(header.dev_minor)}{hex_text(header.rdev_major)}{hex_text(header.rdev_minor)}{hex_text(name_size)}{hex_text(header.checksum)}"
    let pad = padding(format, NEWC_HEADER + name_size)
    return bytes.concat([bytes.from_text(text), name, ZERO_PAD.slice(0, 1 + pad)])
  }
  let dev = make_device(header.dev_major, header.dev_minor)
  let rdev = make_device(header.rdev_major, header.rdev_minor)
  if format == "odc" or format == "hpodc" {
    var stored_dev = dev
    var stored_rdev = rdev
    var stored_size = header.size
    if format == "hpodc" {
      let kind = kind_bits(header.mode)
      if kind == TYPE_CHAR or kind == TYPE_BLOCK or kind == TYPE_FIFO or kind == TYPE_SOCKET {
        stored_size = rdev
        stored_dev = make_device(0, 1)
        stored_rdev = make_device(0, 1)
      }
    }
    if ! fits_octal(name_size, 6) {
      gnu.error(f"{shown_name}: value name size {name_size} out of allowed range 0..262143")
      return null
    }
    if ! fits_octal(stored_size, 11) {
      gnu.error(f"{shown_name}: value file size {stored_size} out of allowed range 0..8589934591")
      return null
    }
    if truncate_warnings {
      let narrow = [
        {label: "device number", value: stored_dev, digits: 6},
        {label: "inode number", value: header.ino, digits: 6},
        {label: "file mode", value: header.mode, digits: 6},
        {label: "uid", value: header.uid, digits: 6},
        {label: "gid", value: header.gid, digits: 6},
        {label: "number of links", value: header.nlink, digits: 6},
        {label: "rdev", value: stored_rdev, digits: 6},
        {label: "modification time", value: header.mtime, digits: 11},
      ]
      for field in narrow {
        if ! fits_octal(field.value, field.digits) { gnu.error(f"{shown_name}: truncating {field.label}") }
      }
    }
    let text = f"070707{octal_text(stored_dev, 6)}{octal_text(header.ino, 6)}{octal_text(header.mode, 6)}{octal_text(header.uid, 6)}{octal_text(header.gid, 6)}{octal_text(header.nlink, 6)}{octal_text(stored_rdev, 6)}{octal_text(header.mtime, 11)}{octal_text(name_size, 6)}{octal_text(stored_size, 11)}"
    return bytes.concat([bytes.from_text(text), name, b"\0"])
  }
  var stored_rdev = rdev
  var stored_size = header.size
  if format == "hpbin" {
    let kind = kind_bits(header.mode)
    if kind == TYPE_CHAR or kind == TYPE_BLOCK or kind == TYPE_FIFO or kind == TYPE_SOCKET {
      stored_size = rdev
      stored_rdev = make_device(0, 1)
    }
  }
  if name_size > 65535 {
    gnu.error(f"{shown_name}: value name size {name_size} out of allowed range 0..65535")
    return null
  }
  if stored_size > 4294967295 {
    gnu.error(f"{shown_name}: value file size {name_size} out of allowed range 0..4294967295")
    return null
  }
  if truncate_warnings {
    let narrow = [
      {label: "inode number", value: header.ino},
      {label: "file mode", value: header.mode},
      {label: "uid", value: header.uid},
      {label: "gid", value: header.gid},
      {label: "number of links", value: header.nlink},
    ]
    for field in narrow {
      if field.value > 65535 { gnu.error(f"{shown_name}: truncating {field.label}") }
    }
  }
  let dev_word = if format == "hpbin" { make_device(header.dev_major, header.dev_minor) } else { dev }
  let words = [
    0o070707, dev_word, header.ino, header.mode, header.uid, header.gid, header.nlink, stored_rdev,
    header.mtime / 65536, header.mtime % 65536, name_size, stored_size / 65536, stored_size % 65536,
  ]
  var chunks: List[Bytes] = [le_word(word) for word in words]
  chunks += [name, ZERO_PAD.slice(0, 1 + name_size % 2)]
  bytes.concat(chunks)
}

# ---------------------------------------------------------------------------
# Names
# ---------------------------------------------------------------------------

# GNU's safer_name_suffix: without absolute names, everything up to and
# including the last ".." component and all leading slashes is dropped; an empty
# remainder becomes ".". Optionally, leading "./" pairs are stripped too.
pure safer_name(name: Bytes, absolute: Bool, strip_dots: Bool) -> Safer {
  var start = 0
  if ! absolute {
    var cut = 0
    var scan = 0
    let total = name.len()
    while scan < total {
      if name.byte_at(scan) == 46 and name.byte_at(scan + 1) == 46 and (scan + 2 >= total or name.byte_at(scan + 2) == 47) {
        cut = scan + 2
      }
      while true {
        let byte = name.byte_at(scan) ?? 0
        scan += 1
        if byte == 47 { break }
        if scan >= total { break }
      }
    }
    start = cut
    while start < total and name.byte_at(start) == 47 { start += 1 }
  }
  let prefix = name.slice(0, start)
  var rest = name.slice(start, name.len() - start)
  var substituted = false
  if rest.is_empty() {
    substituted = start == 0
    rest = b"."
  }
  if strip_dots and rest != b"./" {
    while rest.byte_at(0) == 46 and rest.byte_at(1) == 47 {
      var skip = 1
      while rest.byte_at(skip) == 47 { skip += 1 }
      rest = rest.slice(skip, rest.len() - skip)
    }
  }
  {name: rest, prefix: prefix, substituted: substituted}
}

# Applies `safer_name` and reports a removed prefix once per distinct prefix.
proc safe_name(st: Run, name: Bytes, absolute: Bool, strip_dots: Bool) [process, env] -> Named {
  let result = safer_name(name, absolute, strip_dots)
  var updated = st
  if result.prefix.len() > 0 {
    var seen = false
    for known in st.prefixes {
      if known == result.prefix { seen = true }
    }
    if ! seen {
      gnu.error(f"Removing leading `{shown(result.prefix)}' from member names")
      updated = {...st, prefixes: st.prefixes + [result.prefix]}
    }
  }
  if result.substituted {
    gnu.error("Substituting `.' for empty member name")
  }
  {name: result.name, st: updated}
}

pure strip_trailing_slashes(name: Bytes) -> Bytes {
  var length = name.len()
  while length > 1 and name.byte_at(length - 1) == 47 { length -= 1 }
  name.slice(0, length)
}

# Splits a name list read from standard input; the last name needs no
# terminator. GNU cpio reads names as C strings, so a NUL inside a name ends it.
pure split_names(data: Bytes, terminator: Int) -> List[Bytes] {
  match data.utf8() {
    Ok(text) => {
      var names: List[Bytes] = []
      let parts = text.split(if terminator == 0 { "\0" } else { "\n" })
      for index in range(parts.len()) {
        if index == parts.len() - 1 and parts[index] == "" { break }
        let cstring = parts[index].find("\0") ?? -1
        names += [bytes.from_text(if cstring < 0 { parts[index] } else { parts[index].byte_slice(0, length: cstring) })]
      }
      names
    }
    Err(_) => {
      var names: List[Bytes] = []
      var begin = 0
      for index in range(data.len()) {
        if data.byte_at(index) == terminator {
          names += [cut_at_nul(data.slice(begin, index - begin))]
          begin = index + 1
        }
      }
      if begin < data.len() { names += [cut_at_nul(data.slice(begin, data.len() - begin))] }
      names
    }
  }
}

pure cut_at_nul(name: Bytes) -> Bytes {
  for index in range(name.len()) {
    if name.byte_at(index) == 0 { return name.slice(0, index) }
  }
  name
}

# ---------------------------------------------------------------------------
# Pattern matching (fnmatch without FNM_PATHNAME or FNM_PERIOD)
# ---------------------------------------------------------------------------

pure class_matches(class: Str, byte: Int) -> Bool {
  match class {
    "alpha" => (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
    "digit" => byte >= 48 and byte <= 57
    "alnum" => (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 48 and byte <= 57)
    "upper" => byte >= 65 and byte <= 90
    "lower" => byte >= 97 and byte <= 122
    "space" => byte == 32 or (byte >= 9 and byte <= 13)
    "blank" => byte == 32 or byte == 9
    "punct" => (byte >= 33 and byte <= 47) or (byte >= 58 and byte <= 64) or (byte >= 91 and byte <= 96) or (byte >= 123 and byte <= 126)
    "print" => byte >= 32 and byte <= 126
    "graph" => byte >= 33 and byte <= 126
    "cntrl" => byte < 32 or byte == 127
    "xdigit" => hex_digit(byte) >= 0
    _ => false
  }
}

# Matches a bracket expression that starts at `start` (the byte after '[').
# An unterminated expression is not valid and the '[' is then literal.
pure bracket_match(pattern: Bytes, start: Int, byte: Int) -> Bracket {
  var at = start
  var negate = false
  let first = pattern.byte_at(at) ?? 0
  if first == 33 or first == 94 {
    negate = true
    at += 1
  }
  var matched = false
  var open = true
  var begin = true
  while at < pattern.len() {
    let current = pattern.byte_at(at) ?? 0
    if current == 93 and ! begin {
      let found = matched != negate
      return {valid: true, matched: found, next: at + 1}
    }
    begin = false
    if current == 91 and pattern.byte_at(at + 1) == 58 {
      var close = at + 2
      while close + 1 < pattern.len() and ! (pattern.byte_at(close) == 58 and pattern.byte_at(close + 1) == 93) { close += 1 }
      if close + 1 < pattern.len() {
        let name = pattern.slice(at + 2, close - at - 2).utf8() ?? ""
        if class_matches(name, byte) { matched = true }
        at = close + 2
        continue
      }
    }
    var low = current
    if current == 92 and at + 1 < pattern.len() {
      at += 1
      low = pattern.byte_at(at) ?? 0
    }
    if pattern.byte_at(at + 1) == 45 and at + 2 < pattern.len() and pattern.byte_at(at + 2) != 93 {
      var high = pattern.byte_at(at + 2) ?? 0
      var advance = 3
      if high == 92 and at + 3 < pattern.len() {
        high = pattern.byte_at(at + 3) ?? 0
        advance = 4
      }
      if byte >= low and byte <= high { matched = true }
      at += advance
    } else {
      if byte == low { matched = true }
      at += 1
    }
  }
  open = false
  {valid: open, matched: false, next: start}
}

# fnmatch(pattern, name, 0): '*' and '?' also match '/' and a leading '.'.
pure fnmatch(pattern: Bytes, name: Bytes) -> Bool {
  var p = 0
  var s = 0
  var star = -1
  var star_s = 0
  let plen = pattern.len()
  let slen = name.len()
  while s < slen {
    let byte = name.byte_at(s) ?? 0
    var advanced = false
    if p < plen {
      let current = pattern.byte_at(p) ?? 0
      if current == 42 {
        star = p
        star_s = s
        p += 1
        continue
      }
      if current == 63 {
        p += 1
        s += 1
        advanced = true
      } else if current == 91 {
        let bracket = bracket_match(pattern, p + 1, byte)
        if bracket.valid {
          if bracket.matched {
            p = bracket.next
            s += 1
            advanced = true
          }
        } else if byte == 91 {
          p += 1
          s += 1
          advanced = true
        }
      } else if current == 92 {
        if p + 1 < plen and pattern.byte_at(p + 1) == byte {
          p += 2
          s += 1
          advanced = true
        }
      } else if current == byte {
        p += 1
        s += 1
        advanced = true
      }
    }
    if advanced { continue }
    if star < 0 { return false }
    p = star + 1
    star_s += 1
    s = star_s
  }
  while p < plen and pattern.byte_at(p) == 42 { p += 1 }
  p == plen
}

# ---------------------------------------------------------------------------
# Listing
# ---------------------------------------------------------------------------

pure type_letter(mode: Int) -> Str {
  let kind = kind_bits(mode)
  if kind == TYPE_FILE {
    "-"
  } else if kind == TYPE_DIR {
    "d"
  } else if kind == TYPE_CHAR {
    "c"
  } else if kind == TYPE_BLOCK {
    "b"
  } else if kind == TYPE_FIFO {
    "p"
  } else if kind == TYPE_LINK {
    "l"
  } else if kind == TYPE_SOCKET {
    "s"
  } else {
    "?"
  }
}

pure triad(mode: Int, shift: Int, special: Int, set_letter: Str, unset_letter: Str) -> Str {
  let divisor = if shift == 6 { 64 } else if shift == 3 { 8 } else { 1 }
  let bits = mode / divisor % 8
  let read = if bits.bit_and(4) != 0 { "r" } else { "-" }
  let write = if bits.bit_and(2) != 0 { "w" } else { "-" }
  let execute = bits.bit_and(1) != 0
  let last = if mode.bit_and(special) != 0 {
    if execute { set_letter } else { unset_letter }
  } else if execute {
    "x"
  } else {
    "-"
  }
  f"{read}{write}{last}"
}

pure mode_string(mode: Int) -> Str {
  f"{type_letter(mode)}{triad(mode, 6, 0o4000, "s", "S")}{triad(mode, 3, 0o2000, "s", "S")}{triad(mode, 0, 0o1000, "t", "T")}"
}

# GNU truncates a name to eight columns in the listing.
pure clip(name: Str) -> Str {
  if name.byte_len() > 8 { name.byte_slice(0, length: 8) } else { name }
}

pure pad_right(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result += " " }
  result
}

proc owner_name(uid: Int, numeric: Bool) [fs] -> Str {
  if numeric { return f"{uid}" }
  match user.by_uid(uid) {
    Ok(account) => account.name
    Err(_) => f"{uid}"
  }
}

proc group_label(gid: Int, numeric: Bool) [fs] -> Str {
  if numeric { return f"{gid}" }
  match group.by_gid(gid) {
    Ok(entry) => entry.name
    Err(_) => f"{gid}"
  }
}

# One `cpio -tv` line. Times older than half a year, or in the future, show
# the year instead of the time of day, like ls.
proc long_line(header: Header, link_target: Bytes?, numeric: Bool, now_seconds: Int) [fs, time, process, env] -> Bytes {
  let who = pad_right(clip(owner_name(header.uid, numeric)), 8)
  let whom = pad_right(clip(group_label(header.gid, numeric)), 8)
  let kind = kind_bits(header.mode)
  let size_text = if kind == TYPE_CHAR or kind == TYPE_BLOCK {
    f"{header.rdev_major:>3}, {header.rdev_minor:>3}"
  } else {
    f"{header.size:>8}"
  }
  let calendar = match time.to_calendar(header.mtime * 1000000000, utc: false) {
    Ok(value) => value
    Err(failure) => {
      die(gnu.strerror(failure))
      exit 2
    }
  }
  let recent = header.mtime > now_seconds - HALF_YEAR_SECONDS and header.mtime < now_seconds
  let stamp = if recent {
    f"{MONTHS[calendar.month - 1]} {calendar.day:>2} {calendar.hour:02}:{calendar.minute:02}"
  } else {
    f"{MONTHS[calendar.month - 1]} {calendar.day:>2}  {calendar.year}"
  }
  let prefix = f"{mode_string(header.mode)} {header.nlink:>3} {who} {whom} {size_text} {stamp} "
  var chunks = [bytes.from_text(prefix), header.name]
  if let target = link_target {
    chunks += [b" -> ", target]
  }
  chunks += [b"\n"]
  bytes.concat(chunks)
}

# ---------------------------------------------------------------------------
# Filesystem helpers shared by extraction and pass-through
# ---------------------------------------------------------------------------

pure fresh_run() -> Run {
  {failed: false, prefixes: [], inodes: {}, deferred: [], placeholders: [], fixups: [], copied: 0, warned_reverse: false}
}

# `NAME: Cannot CALL: STRERROR`, which marks the run failed (GNU's ERROR class).
proc call_error(st: Run, call: Str, name: Bytes, failure: Error) [process, env] -> Run {
  gnu.error(f"{shown(name)}: Cannot {call}: {gnu.strerror(failure)}")
  {...st, failed: true}
}

pure path_of(name: Bytes) -> Result[Path, Error] {
  Path.parse_bytes(name)
}

pure hex_plain(value: Int) -> Str {
  if value == 0 { return "0" }
  var left = value
  var text = ""
  while left > 0 {
    text = f"{"0123456789abcdef".byte_slice(left % 16, length: 1)}{text}"
    left = left / 16
  }
  text
}

pure inode_key(header: Header) -> Str {
  f"{header.ino}:{header.dev_major}:{header.dev_minor}"
}

pure same_inode(left: Header, right: Header) -> Bool {
  left.ino == right.ino and left.dev_major == right.dev_major and left.dev_minor == right.dev_minor
}

pure type_bits_of(kind: Str) -> Int {
  match kind {
    "file" => TYPE_FILE
    "dir" => TYPE_DIR
    "symlink" => TYPE_LINK
    "fifo" => TYPE_FIFO
    "char" => TYPE_CHAR
    "block" => TYPE_BLOCK
    "socket" => TYPE_SOCKET
    _ => 0
  }
}

pure kind_of_bits(bits: Int) -> Str {
  if bits == TYPE_CHAR {
    "char"
  } else if bits == TYPE_BLOCK {
    "block"
  } else if bits == TYPE_FIFO {
    "fifo"
  } else if bits == TYPE_SOCKET {
    "socket"
  } else {
    ""
  }
}

# Dots for -V go to standard error without a newline, flushed so they show at once.
proc progress(text: Str) [io] -> Unit {
  let _ = io.write_stderr(text)
  let _ = io.flush_stderr()
}

proc block_count_text(bytes_seen: Int, block_size: Int) -> Str {
  let blocks = (bytes_seen + block_size - 1) / block_size
  if blocks == 1 { "1 block" } else { f"{blocks} blocks" }
}

# Makes every missing parent directory of `file`, shallowest first. Under
# -W interdir each directory made is announced; failures use GNU's wording.
proc create_parents(st: Run, opts: Options, file: Path) [fs, process, env] -> Created {
  var missing: List[Path] = []
  var current = file.parent()
  while true {
    let text = current.display()
    if text == "" or text == "." or text == "/" { break }
    match fs.stat(current) {
      Ok(_) => { break }
      Err(_) => {
        missing = [current] + missing
        current = current.parent()
      }
    }
  }
  for directory in missing {
    match directory.mkdir() {
      Ok(_) => {
        if opts.warn_interdir { gnu.error(f"Creating intermediate directory `{directory.display()}'") }
      }
      Err(failure) => {
        if gnu.errno(failure) != 17 {
          gnu.error(f"cannot make directory `{directory.display()}': {gnu.strerror(failure)}")
          return {ok: false, st: st}
        }
      }
    }
  }
  {ok: true, st: st}
}

# Hard-links `target` as `name`; under -d a missing parent directory is made.
proc link_to_name(st: Run, opts: Options, name: Bytes, target: Path) [fs, process, env] -> Linked {
  let file = match path_of(name) {
    Ok(value) => value
    Err(failure) => { return {linked: false, reason: gnu.strerror(failure), st: st} }
  }
  var result = fs.link(target, file)
  var state = st
  if result is Err(_) and opts.make_dirs {
    let made = create_parents(state, opts, file)
    state = made.st
    result = fs.link(target, file)
  }
  match result {
    Ok(_) => {
      if opts.verbose { gnu.error(f"{target.display()} linked to {shown(name)}") }
      {linked: true, reason: "", st: state}
    }
    Err(failure) => {
      let reason = gnu.strerror(failure)
      if opts.link { gnu.error(f"cannot link {target.display()} to {shown(name)}: {reason}") }
      {linked: false, reason: reason, st: state}
    }
  }
}

# If a file with the same inode was already created, links `name` to it;
# otherwise remembers `name` as that inode's first file.
proc link_to_inode(st: Run, opts: Options, name: Bytes, header: Header) [fs, process, env] -> Linked {
  let key = inode_key(header)
  match st.inodes.get(key) {
    Ok(existing) => {
      return link_to_name(st, opts, name, existing)
    }
    Err(_) => {}
  }
  match path_of(name) {
    Ok(file) => {
      return {linked: false, reason: "", st: {...st, inodes: st.inodes.set(key, file)}}
    }
    Err(_) => {}
  }
  {linked: false, reason: "", st: st}
}

# Gives `file` the archive's owner (or the -R owner). Only a privileged run, or
# one with -R, tries; EPERM is ignored silently, as GNU cpio does.
proc apply_owner(st: Run, opts: Options, file: Path, name: Bytes, uid: Int, gid: Int, follow: Bool, chown_enabled: Bool) [fs, process, env] -> Run {
  if ! chown_enabled { return st }
  let want_uid = opts.owner ?? uid
  let want_gid = opts.group ?? gid
  match fs.set_owner(file, uid: want_uid, gid: want_gid, follow_symlinks: follow) {
    Ok(_) => st
    Err(failure) => {
      if gnu.errno(failure) == 1 { return st }
      gnu.error(f"{shown(name)}: Cannot change ownership to uid {want_uid}, gid {want_gid}: {gnu.strerror(failure)}")
      {...st, failed: true}
    }
  }
}

# Owner, then mode (chown may have cleared bits), then the -m modification time.
proc set_perms(st: Run, opts: Options, file: Path, name: Bytes, mode: Int, uid: Int, gid: Int, mtime: Int, chown_enabled: Bool) [fs, process, env] -> Run {
  var state = apply_owner(st, opts, file, name, uid, gid, true, chown_enabled)
  let bits = mode.bit_and(4095)
  match file.chmod(bits) {
    Ok(_) => {}
    Err(failure) => {
      gnu.error(f"{shown(name)}: Cannot change mode to {mode_string(bits).byte_slice(1)}: {gnu.strerror(failure)}")
      state = {...state, failed: true}
    }
  }
  if opts.preserve_mtime {
    match fs.set_times(file, atime_ns: mtime * 1000000000, mtime_ns: mtime * 1000000000) {
      Ok(_) => {}
      Err(failure) => {
        if gnu.errno(failure) != 30 {
          state = call_error(state, "utime", name, failure)
        }
      }
    }
  }
  state
}

# Applies owner, mode and time to directories left for the end. The directory
# created last is finished first, so a read-only parent does not block a child.
proc apply_fixups(st: Run, opts: Options, chown_enabled: Bool) [fs, process, env] -> Run {
  var state = st
  for index in range(st.fixups.len()) {
    let fixup = st.fixups[st.fixups.len() - 1 - index]
    state = set_perms(state, opts, fixup.path, fixup.name, fixup.mode, fixup.uid, fixup.gid, fixup.mtime, chown_enabled)
  }
  {...state, fixups: []}
}

# -s and -S: swaps bytes of halfwords and halfwords of words. With both, each
# word is reversed. The caller checked that the length fits the swap.
pure swap_data(data: Bytes, halfwords: Bool, swap_bytes: Bool) -> Bytes {
  if ! halfwords and ! swap_bytes { return data }
  var out: List[Int] = []
  let total = data.len()
  if halfwords {
    for word in range(total / 4) {
      let a = data.byte_at(word * 4) ?? 0
      let b = data.byte_at(word * 4 + 1) ?? 0
      let c = data.byte_at(word * 4 + 2) ?? 0
      let d = data.byte_at(word * 4 + 3) ?? 0
      if swap_bytes {
        out += [d, c, b, a]
      } else {
        out += [c, d, a, b]
      }
    }
    return bytes.from_ints(out) ?? data
  }
  for pair in range(total / 2) {
    out += [data.byte_at(pair * 2 + 1) ?? 0, data.byte_at(pair * 2) ?? 0]
  }
  if total % 2 == 1 { out += [data.byte_at(total - 1) ?? 0] }
  bytes.from_ints(out) ?? data
}

# Creates the file with owner-only permissions, as GNU cpio does before the
# final mode is applied. Under --sparse, zero blocks become holes.
proc write_file(file: Path, data: Bytes, sparse: Bool) [fs, error] -> Result[Unit, Error] {
  if ! sparse or data.len() < 512 {
    return file.write(data, 384)
  }
  file.write(b"", 384)?
  let zeros = bytes.zero(512)?
  var offset = 0
  var run_start = -1
  let total = data.len()
  while offset < total {
    let length = if total - offset < 512 { total - offset } else { 512 }
    let hole = length == 512 and data.slice(offset, length) == zeros
    if hole {
      if run_start >= 0 {
        let _ = bytes.write_at(file, run_start, data.slice(run_start, offset - run_start))?
        run_start = -1
      }
    } else if run_start < 0 {
      run_start = offset
    }
    offset += length
  }
  if run_start >= 0 {
    let _ = bytes.write_at(file, run_start, data.slice(run_start, total - run_start))?
  }
  file.truncate(total)
}

# ---------------------------------------------------------------------------
# Option parsing
# ---------------------------------------------------------------------------

type LongOption = {name: Str, key: Str, argument: Bool}

const LONG_OPTIONS: List[LongOption] = [
  {name: "create", key: "o", argument: false},
  {name: "extract", key: "i", argument: false},
  {name: "pass-through", key: "p", argument: false},
  {name: "list", key: "t", argument: false},
  {name: "directory", key: "D", argument: true},
  {name: "force-local", key: "force-local", argument: false},
  {name: "format", key: "H", argument: true},
  {name: "block-size", key: "block-size", argument: true},
  {name: "dot", key: "V", argument: false},
  {name: "io-size", key: "C", argument: true},
  {name: "quiet", key: "quiet", argument: false},
  {name: "verbose", key: "v", argument: false},
  {name: "warning", key: "W", argument: true},
  {name: "owner", key: "R", argument: true},
  {name: "file", key: "F", argument: true},
  {name: "message", key: "M", argument: true},
  {name: "rsh-command", key: "rsh-command", argument: true},
  {name: "nonmatching", key: "f", argument: false},
  {name: "numeric-uid-gid", key: "n", argument: false},
  {name: "pattern-file", key: "E", argument: true},
  {name: "only-verify-crc", key: "only-verify-crc", argument: false},
  {name: "rename", key: "r", argument: false},
  {name: "rename-batch-file", key: "rename-batch-file", argument: true},
  {name: "swap", key: "b", argument: false},
  {name: "swap-bytes", key: "s", argument: false},
  {name: "swap-halfwords", key: "S", argument: false},
  {name: "to-stdout", key: "to-stdout", argument: false},
  {name: "append", key: "A", argument: false},
  {name: "renumber-inodes", key: "renumber-inodes", argument: false},
  {name: "ignore-devno", key: "ignore-devno", argument: false},
  {name: "ignore-dirnlink", key: "ignore-dirnlink", argument: false},
  {name: "device-independent", key: "device-independent", argument: false},
  {name: "reproducible", key: "device-independent", argument: false},
  {name: "link", key: "l", argument: false},
  {name: "absolute-filenames", key: "absolute-filenames", argument: false},
  {name: "no-absolute-filenames", key: "no-absolute-filenames", argument: false},
  {name: "null", key: "0", argument: false},
  {name: "dereference", key: "L", argument: false},
  {name: "reset-access-time", key: "a", argument: false},
  {name: "preserve-modification-time", key: "m", argument: false},
  {name: "make-directories", key: "d", argument: false},
  {name: "no-preserve-owner", key: "no-preserve-owner", argument: false},
  {name: "unconditional", key: "u", argument: false},
  {name: "sparse", key: "sparse", argument: false},
  {name: "help", key: "?", argument: false},
  {name: "usage", key: "usage", argument: false},
  {name: "version", key: "version", argument: false},
]

const SHORT_FLAGS = "ioptBcvVbfnrsSAl0aLdmu?"
const SHORT_ARGUMENTS = "CDHRWFMIOE"

# USER[:.][GROUP] as chown reads it. A bare number is accepted for an id with
# no database entry.
proc parse_owner(spec: Str) [fs, process, env] -> Result[UserSpec, Str] {
  var user_part = spec
  var group_part = ""
  var colon = spec.find(":") ?? -1
  if colon < 0 {
    let dot = spec.find(".") ?? -1
    if dot >= 0 {
      match user.lookup(spec) {
        Ok(_) => {}
        Err(_) => { colon = dot }
      }
    }
  }
  if colon >= 0 {
    user_part = spec.byte_slice(0, length: colon)
    group_part = spec.byte_slice(colon + 1)
  }
  var uid: Int? = null
  var gid: Int? = null
  if user_part != "" {
    match user.lookup(user_part) {
      Ok(account) => {
        uid = account.uid
        if colon >= 0 and group_part == "" { gid = account.gid }
      }
      Err(_) => {
        match user_part.parse_int_decimal() {
          Ok(number) => { uid = number }
          Err(_) => { return Err("invalid user") }
        }
      }
    }
  }
  if group_part != "" {
    match group.lookup(group_part) {
      Ok(entry) => { gid = entry.gid }
      Err(_) => {
        match group_part.parse_int_decimal() {
          Ok(number) => { gid = number }
          Err(_) => { return Err("invalid group") }
        }
      }
    }
  }
  Ok({uid: uid, gid: gid})
}

# atoi: the leading decimal digits, or 0 when there are none.
pure parse_block_count(value: Str) -> Int {
  var position = 0
  while position < value.byte_len() and value.byte_slice(position, length: 1) == " " { position += 1 }
  var negative = false
  if position < value.byte_len() and value.byte_slice(position, length: 1) in ["+", "-"] {
    negative = value.byte_slice(position, length: 1) == "-"
    position += 1
  }
  var number = 0
  while position < value.byte_len() {
    let digit = hex_digit(value.byte_at(position) ?? 0)
    if digit < 0 or digit > 9 { break }
    number = number * 10 + digit
    position += 1
  }
  if negative { -number } else { number }
}

proc choose_mode(opts: Options, mode: Str) [process, env] -> Options {
  if opts.mode != "" { usage_failure("Mode already defined", 2) }
  {...opts, mode: mode}
}

proc choose_format(opts: Options, format: Str) [process, env] -> Options {
  if opts.format != "" { usage_failure("Archive format multiply defined", 2) }
  {...opts, format: format}
}

proc apply_warning(opts: Options, flag: Str) [process, env] -> Options {
  if flag == "none" { return {...opts, warn_truncate: false, warn_interdir: false} }
  var negate = false
  var name = flag
  if flag.byte_len() > 2 and flag.starts_with("no-") {
    negate = true
    name = flag.byte_slice(3)
  }
  match name {
    "truncate" => {...opts, warn_truncate: ! negate}
    "interdir" => {...opts, warn_interdir: ! negate}
    "all" => {...opts, warn_truncate: ! negate, warn_interdir: ! negate}
    _ => {
      usage_failure(f"Invalid value for --warning option: {flag}", 2)
      opts
    }
  }
}

proc apply_option(opts: Options, key: Str, value: Str) [fs, process, env] -> Options {
  match key {
    "o" => choose_mode(opts, "out")
    "i" => choose_mode(opts, "in")
    "p" => choose_mode(opts, "pass")
    "t" => {...opts, table: true}
    "D" => {...opts, directory: value}
    "force-local" => {...opts, force_local: true}
    "H" => {
      let lowered = value.lower()
      if ! (lowered in ["crc", "newc", "odc", "bin", "ustar", "tar", "hpodc", "hpbin"]) {
        usage_failure(f"invalid archive format `{value}'; valid formats are:\ncrc newc odc bin ustar tar (all-caps also recognized)", 2)
      }
      if lowered == "tar" or lowered == "ustar" {
        usage_failure(f"archive format `{value}' is not supported", 64)
      }
      choose_format(opts, lowered)
    }
    "B" => {...opts, block_size: 5120}
    "block-size" => {
      let count = parse_block_count(value)
      if count < 1 or count > 4194303 { usage_failure("invalid block size", 2) }
      {...opts, block_size: count * 512}
    }
    "c" => choose_format(opts, "odc")
    "V" => {...opts, dot: true}
    "C" => {
      let count = parse_block_count(value)
      if count < 1 { usage_failure("invalid block size", 2) }
      {...opts, block_size: count}
    }
    "quiet" => {...opts, quiet: true}
    "v" => {...opts, verbose: true}
    "W" => apply_warning(opts, value)
    "R" => {
      if opts.no_preserve_owner { usage_failure("--owner cannot be used with --no-preserve-owner", 2) }
      match parse_owner(value) {
        Ok(spec) => {
          var updated = opts
          if let uid = spec.uid { updated = {...updated, owner: uid} }
          if let gid = spec.gid { updated = {...updated, group: gid} }
          updated
        }
        Err(reason) => {
          usage_failure(f"{value}: {reason}", 2)
          opts
        }
      }
    }
    "F" => {...opts, archive: value}
    "M" => {
      usage_failure("option '--message' is not supported: archives are never split across volumes", 64)
      opts
    }
    "rsh-command" => {
      usage_failure("option '--rsh-command' is not supported: remote archives are not available", 64)
      opts
    }
    "f" => {...opts, nonmatching: true}
    "n" => {...opts, numeric: true}
    "E" => {...opts, pattern_file: value}
    "only-verify-crc" => {...opts, only_verify_crc: true}
    "r" => {...opts, rename: true}
    "rename-batch-file" => {...opts, rename_batch: value}
    "b" => {...opts, swap_bytes: true, swap_halfwords: true}
    "s" => {...opts, swap_bytes: true}
    "S" => {...opts, swap_halfwords: true}
    "to-stdout" => {...opts, to_stdout: true}
    "I" => {...opts, input_archive: value}
    "A" => {...opts, append: true}
    "O" => {...opts, output_archive: value}
    "renumber-inodes" => {...opts, renumber: true}
    "ignore-devno" => {...opts, ignore_devno: true}
    "ignore-dirnlink" => {...opts, ignore_dirnlink: true}
    "device-independent" => {...opts, renumber: true, ignore_devno: true, ignore_dirnlink: true}
    "l" => {...opts, link: true}
    "absolute-filenames" => {...opts, no_abs: false}
    "no-absolute-filenames" => {...opts, no_abs: true}
    "0" => {...opts, null: true}
    "L" => {...opts, dereference: true}
    "a" => {...opts, reset_time: true}
    "m" => {...opts, preserve_mtime: true}
    "d" => {...opts, make_dirs: true}
    "no-preserve-owner" => {
      if opts.owner != null or opts.group != null { usage_failure("--no-preserve-owner cannot be used with --owner", 2) }
      {...opts, no_preserve_owner: true}
    }
    "u" => {...opts, unconditional: true}
    "sparse" => {...opts, sparse: true}
    "?" => {...opts, help: true}
    "usage" => {...opts, usage: true}
    "version" => {...opts, version: true}
    _ => opts
  }
}

# argp's grammar: bundled short flags, long options by unambiguous prefix,
# --name=value or --name value. Errors use argp wording and status 64.
proc parse_command_line(argv: List[Str]) [fs, process, env] -> Options {
  var opts: Options = {
    mode: "", table: false, verbose: false, dot: false, quiet: false, format: "", block_size: 512,
    directory: null, force_local: false, owner: null, group: null, warn_truncate: false,
    warn_interdir: false, archive: null, input_archive: null, output_archive: null,
    swap_bytes: false, swap_halfwords: false, nonmatching: false, numeric: false,
    pattern_file: null, only_verify_crc: false, rename: false, rename_batch: null, to_stdout: false,
    append: false, renumber: false, ignore_devno: false, ignore_dirnlink: false, link: false,
    no_abs: false, null: false, reset_time: false, dereference: false, make_dirs: false,
    preserve_mtime: false, no_preserve_owner: false, sparse: false, unconditional: false,
    help: false, usage: false, version: false, operands: [],
  }
  var index = 0
  var only_operands = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if only_operands or word == "-" or ! word.starts_with("-") {
      opts = {...opts, operands: opts.operands + [word]}
      continue
    }
    if word == "--" {
      only_operands = true
      continue
    }
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let equals = body.find("=") ?? -1
      let given = if equals < 0 { body } else { body.byte_slice(0, length: equals) }
      var inline_value: Str? = null
      if equals >= 0 { inline_value = body.byte_slice(equals + 1) }
      var chosen: LongOption? = null
      var candidates: List[LongOption] = []
      for option in LONG_OPTIONS {
        if option.name == given { chosen = option }
        if option.name.starts_with(given) { candidates += [option] }
      }
      if chosen == null {
        var distinct: List[LongOption] = []
        for option in candidates {
          var known = false
          for other in distinct {
            if other.key == option.key { known = true }
          }
          if ! known { distinct += [option] }
        }
        if distinct.is_empty() { usage_failure(f"unrecognized option '--{given}'", 64) }
        if distinct.len() > 1 {
          var names = ""
          for option in candidates { names += f" '--{option.name}'" }
          usage_failure(f"option '--{given}' is ambiguous; possibilities:{names}", 64)
        }
        chosen = distinct[0]
      }
      let option = chosen ?? LONG_OPTIONS[0]
      var value = ""
      if option.argument {
        if let attached = inline_value {
          value = attached
        } else {
          if index >= argv.len() { usage_failure(f"option '--{option.name}' requires an argument", 64) }
          value = argv[index]
          index += 1
        }
      } else if inline_value != null {
        usage_failure(f"option '--{option.name}' doesn't allow an argument", 64)
      }
      opts = apply_option(opts, option.key, value)
      continue
    }
    var position = 1
    while position < word.byte_len() {
      let letter = word.byte_slice(position, length: 1)
      position += 1
      if SHORT_ARGUMENTS.find(letter) != null {
        var value = word.byte_slice(position)
        position = word.byte_len()
        if value == "" {
          if index >= argv.len() { usage_failure(f"option requires an argument -- '{letter}'", 64) }
          value = argv[index]
          index += 1
        }
        opts = apply_option(opts, letter, value)
      } else if SHORT_FLAGS.find(letter) != null {
        opts = apply_option(opts, letter, "")
      } else {
        usage_failure(f"invalid option -- '{letter}'", 64)
      }
    }
  }
  opts
}

# Option combinations that are meaningless for the chosen mode.
proc meaningless(opts: Options, active: Bool, flag: Str, mode_flag: Str) [process, env] -> Unit {
  if active { usage_failure(f"{flag} is meaningless with {mode_flag}", 2) }
}

# Mirrors GNU cpio's checks after parsing; returns the options for the mode.
proc validate_options(opts: Options) [process, env] -> Options {
  var checked = opts
  if opts.mode == "" {
    if opts.table {
      checked = {...checked, mode: "in"}
    } else {
      usage_failure("You must specify one of -oipt options.", 2)
    }
  }
  if checked.mode == "in" {
    meaningless(checked, checked.link, "--link", "--extract")
    meaningless(checked, checked.reset_time, "--reset", "--extract")
    meaningless(checked, checked.dereference, "--dereference", "--extract")
    meaningless(checked, checked.append, "--append", "--extract")
    meaningless(checked, checked.output_archive != null, "-O", "--extract")
    meaningless(checked, checked.renumber, "--renumber-inodes", "--extract")
    meaningless(checked, checked.ignore_devno, "--ignore-devno", "--extract")
    if checked.to_stdout {
      meaningless(checked, checked.make_dirs, "--make-directories", "--to-stdout")
      meaningless(checked, checked.rename, "--rename", "--to-stdout")
      meaningless(checked, checked.no_preserve_owner, "--no-preserve-owner", "--to-stdout")
      meaningless(checked, checked.owner != null or checked.group != null, "--owner", "--to-stdout")
      meaningless(checked, checked.preserve_mtime, "--preserve-modification-time", "--to-stdout")
    }
    if checked.archive != null and checked.input_archive != null {
      usage_failure("Both -I and -F are used in copy-in mode", 2)
    }
    if checked.input_archive != null { checked = {...checked, archive: checked.input_archive} }
  } else if checked.mode == "out" {
    if ! checked.operands.is_empty() { usage_failure("Too many arguments", 2) }
    meaningless(checked, checked.make_dirs, "--make-directories", "--create")
    meaningless(checked, checked.rename, "--rename", "--create")
    meaningless(checked, checked.table, "--list", "--create")
    meaningless(checked, checked.unconditional, "--unconditional", "--create")
    meaningless(checked, checked.link, "--link", "--create")
    meaningless(checked, checked.sparse, "--sparse", "--create")
    meaningless(checked, checked.preserve_mtime, "--preserve-modification-time", "--create")
    meaningless(checked, checked.no_preserve_owner, "--no-preserve-owner", "--create")
    meaningless(checked, checked.swap_bytes, "--swap-bytes (--swap)", "--create")
    meaningless(checked, checked.swap_halfwords, "--swap-halfwords (--swap)", "--create")
    meaningless(checked, checked.to_stdout, "--to-stdout", "--create")
    if checked.append and checked.archive == null and checked.output_archive == null {
      usage_failure("--append is used but no archive file name is given (use -F or -O options)", 2)
    }
    meaningless(checked, checked.rename_batch != null, "--rename-batch-file", "--create")
    meaningless(checked, checked.input_archive != null, "-I", "--create")
    if checked.archive != null and checked.output_archive != null {
      usage_failure("Both -O and -F are used in copy-out mode", 2)
    }
    if checked.format == "" { checked = {...checked, format: "bin"} }
    if checked.output_archive != null { checked = {...checked, archive: checked.output_archive} }
  } else {
    if checked.operands.len() > 1 { usage_failure("Too many arguments", 2) }
    if checked.operands.is_empty() { usage_failure("Not enough arguments", 2) }
    if checked.format != "" {
      usage_failure("Archive format is not specified in copy-pass mode (use --format option)", 2)
    }
    meaningless(checked, checked.swap_bytes, "--swap-bytes (--swap)", "--pass-through")
    meaningless(checked, checked.swap_halfwords, "--swap-halfwords (--swap)", "--pass-through")
    meaningless(checked, checked.table, "--list", "--pass-through")
    meaningless(checked, checked.rename, "--rename", "--pass-through")
    meaningless(checked, checked.append, "--append", "--pass-through")
    meaningless(checked, checked.rename_batch != null, "--rename-batch-file", "--pass-through")
    meaningless(checked, checked.no_abs, "--no-absolute-pathnames", "--pass-through")
    meaningless(checked, checked.to_stdout, "--to-stdout", "--pass-through")
    meaningless(checked, checked.renumber, "--renumber-inodes", "--pass-through")
    meaningless(checked, checked.ignore_devno, "--ignore-devno", "--pass-through")
    if checked.archive != null {
      die("-F can be used only with --create or --extract")
    }
  }
  checked
}

# ---------------------------------------------------------------------------
# Copy-in: list and extract
# ---------------------------------------------------------------------------

# GNU cpio_create_dir. A directory the archive makes unwritable, or one whose
# time must survive its contents being created, is finished at the end.
proc create_directory(st: Run, opts: Options, header: Header, existing: Bool, chown_enabled: Bool) [fs, process, env] -> Named {
  if opts.to_stdout { return {name: header.name, st: st} }
  let name = strip_trailing_slashes(header.name)
  if name == b"." { return {name: name, st: st} }
  let file = match path_of(name) {
    Ok(value) => value
    Err(failure) => { return {name: name, st: call_error(st, "mkdir", name, failure)} }
  }
  var state = st
  if ! existing {
    var result = file.mkdir()
    if result is Err(_) and opts.make_dirs {
      let made = create_parents(state, opts, file)
      state = made.st
      result = file.mkdir()
    }
    match result {
      Ok(_) => {}
      Err(failure) => {
        if gnu.errno(failure) != 17 {
          return {name: name, st: call_error(state, "mkdir", name, failure)}
        }
        match fs.stat(file, follow_symlinks: false) {
          Ok(info) => {
            if info.kind != "dir" {
              gnu.error(f"{shown(name)} is not a directory")
              return {name: name, st: state}
            }
          }
          Err(problem) => { return {name: name, st: call_error(state, "stat", name, problem)} }
        }
      }
    }
  }
  if header.mode.bit_and(0o200) != 0 and ! opts.preserve_mtime {
    state = set_perms(state, opts, file, name, header.mode, header.uid, header.gid, header.mtime, chown_enabled)
  } else {
    let fixup: DirFixup = {path: file, name: name, mode: header.mode, uid: header.uid, gid: header.gid, mtime: header.mtime}
    state = {...state, fixups: state.fixups + [fixup]}
  }
  {name: name, st: state}
}

# After a data-carrying link is created, links every deferred name for the same inode to it.
proc create_deferred_links(st: Run, opts: Options, header: Header) [fs, process, env] -> Run {
  var state = st
  var remaining: List[Header] = []
  let target = match path_of(header.name) {
    Ok(value) => value
    Err(_) => { return st }
  }
  for waiting in st.deferred {
    if same_inode(waiting, header) {
      let linked = link_to_name(state, opts, waiting.name, target)
      state = linked.st
      if ! linked.linked {
        gnu.error(f"cannot link {shown(waiting.name)} to {shown(header.name)}: {linked.reason}")
      }
    } else {
      remaining += [waiting]
    }
  }
  {...state, deferred: remaining}
}

proc verify_checksum(format: Str, header: Header, content: Bytes) [process, env] -> Unit {
  if format != "crc" { return }
  let sum = byte_sum(content)
  if sum != header.checksum {
    gnu.error(f"{shown(header.name)}: checksum error (0x{hex_plain(sum)}, should be 0x{hex_plain(header.checksum)})")
  }
}

proc extract_regular(st: Run, opts: Options, format: Str, header: Header, content: Bytes, chown_enabled: Bool) [fs, io, error, process, env] -> Run {
  var state = st
  let name = header.name
  if ! opts.to_stdout and header.nlink > 1 {
    if (format == "newc" or format == "crc") and header.size == 0 {
      return {...state, deferred: [header] + state.deferred}
    }
    let linked = link_to_inode(state, opts, name, header)
    state = linked.st
    if linked.linked { return state }
  }
  var swap_words = false
  var swap_pairs = false
  if opts.swap_halfwords {
    if header.size % 4 == 0 {
      swap_words = true
    } else {
      gnu.error(f"cannot swap halfwords of {shown(name)}: odd number of halfwords")
    }
  }
  if opts.swap_bytes {
    if header.size % 2 == 0 {
      swap_pairs = true
    } else {
      gnu.error(f"cannot swap bytes of {shown(name)}: odd number of bytes")
    }
  }
  let output = swap_data(content, swap_words, swap_pairs)
  if opts.to_stdout {
    write_out(output)
    verify_checksum(format, header, content)
    return state
  }
  let file = match path_of(name) {
    Ok(value) => value
    Err(failure) => { return call_error(state, "open", name, failure) }
  }
  var result = write_file(file, output, opts.sparse)
  if result is Err(_) and opts.make_dirs {
    let made = create_parents(state, opts, file)
    state = made.st
    result = write_file(file, output, opts.sparse)
  }
  match result {
    Ok(_) => {}
    Err(failure) => { return call_error(state, "open", name, failure) }
  }
  verify_checksum(format, header, content)
  state = set_perms(state, opts, file, name, header.mode, header.uid, header.gid, header.mtime, chown_enabled)
  if header.nlink > 1 and (format == "newc" or format == "crc") {
    state = create_deferred_links(state, opts, header)
  }
  state
}

proc extract_device(st: Run, opts: Options, header: Header, chown_enabled: Bool) [fs, process, env] -> Run {
  if opts.to_stdout { return st }
  var state = st
  let name = header.name
  if header.nlink > 1 {
    let linked = link_to_inode(state, opts, name, header)
    state = linked.st
    if linked.linked { return state }
  }
  let file = match path_of(name) {
    Ok(value) => value
    Err(failure) => { return call_error(state, "mknod", name, failure) }
  }
  let kind = kind_of_bits(kind_bits(header.mode))
  let permissions = header.mode.bit_and(511)
  let numbered = kind == "char" or kind == "block"
  let major = if numbered { header.rdev_major } else { 0 }
  let minor = if numbered { header.rdev_minor } else { 0 }
  var result = if numbered { fs.mknod(file, kind, permissions, major: major, minor: minor) } else { fs.mknod(file, kind, permissions) }
  if result is Err(_) and opts.make_dirs {
    let made = create_parents(state, opts, file)
    state = made.st
    result = if numbered { fs.mknod(file, kind, permissions, major: major, minor: minor) } else { fs.mknod(file, kind, permissions) }
  }
  match result {
    Ok(_) => {}
    Err(failure) => { return call_error(state, "mknod", name, failure) }
  }
  set_perms(state, opts, file, name, header.mode, header.uid, header.gid, header.mtime, chown_enabled)
}

proc extract_link(st: Run, opts: Options, header: Header, target: Bytes, chown_enabled: Bool) [fs, process, env] -> Run {
  if opts.to_stdout { return st }
  var state = st
  let name = header.name
  let file = match path_of(name) {
    Ok(value) => value
    Err(failure) => { return call_error(state, "open", name, failure) }
  }
  if opts.no_abs {
    var made = file.write(b"", 0)
    if made is Err(_) and opts.make_dirs {
      let parents = create_parents(state, opts, file)
      state = parents.st
      made = file.write(b"", 0)
    }
    match made {
      Ok(_) => {}
      Err(failure) => { return call_error(state, "open", name, failure) }
    }
    match fs.stat(file, follow_symlinks: false) {
      Ok(info) => {
        let placeholder: Placeholder = {path: file, name: name, target: target, mode: header.mode, uid: header.uid, gid: header.gid, mtime: header.mtime, dev: info.dev, ino: info.ino}
        return {...state, placeholders: state.placeholders + [placeholder]}
      }
      Err(failure) => { return call_error(state, "stat", name, failure) }
    }
  }
  let destination = match path_of(target) {
    Ok(value) => value
    Err(failure) => { return call_error(state, "symlink", name, failure) }
  }
  var result = fs.symlink(destination, file)
  if result is Err(_) and opts.make_dirs {
    let parents = create_parents(state, opts, file)
    state = parents.st
    result = fs.symlink(destination, file)
  }
  match result {
    Ok(_) => {}
    Err(failure) => {
      gnu.error(f"{shown(name)}: Cannot create symlink to '{shown(target)}': {gnu.strerror(failure)}")
      return {...state, failed: true}
    }
  }
  state = apply_owner(state, opts, file, name, header.uid, header.gid, false, chown_enabled)
  if opts.preserve_mtime {
    match fs.set_times(file, atime_ns: header.mtime * 1000000000, mtime_ns: header.mtime * 1000000000, follow_symlinks: false) {
      Ok(_) => {}
      Err(failure) => { state = call_error(state, "utime", name, failure) }
    }
  }
  state
}

# Replaces each placeholder still in place with the symbolic link it stands for.
proc replace_placeholders(st: Run, opts: Options, chown_enabled: Bool) [fs, process, env] -> Run {
  var state = st
  for holder in st.placeholders {
    match fs.stat(holder.path, follow_symlinks: false) {
      Ok(info) => {
        if info.dev != holder.dev or info.ino != holder.ino { continue }
      }
      Err(_) => { continue }
    }
    match holder.path.unlink() {
      Ok(_) => {}
      Err(failure) => {
        state = call_error(state, "unlink", holder.name, failure)
        continue
      }
    }
    let destination = match path_of(holder.target) {
      Ok(value) => value
      Err(failure) => {
        state = call_error(state, "symlink", holder.name, failure)
        continue
      }
    }
    var result = fs.symlink(destination, holder.path)
    if result is Err(_) and opts.make_dirs {
      let parents = create_parents(state, opts, holder.path)
      state = parents.st
      result = fs.symlink(destination, holder.path)
    }
    match result {
      Ok(_) => {
        state = apply_owner(state, opts, holder.path, holder.name, holder.uid, holder.gid, false, chown_enabled)
        if opts.preserve_mtime {
          match fs.set_times(holder.path, atime_ns: holder.mtime * 1000000000, mtime_ns: holder.mtime * 1000000000, follow_symlinks: false) {
            Ok(_) => {}
            Err(failure) => { state = call_error(state, "utime", holder.name, failure) }
          }
        }
      }
      Err(failure) => {
        gnu.error(f"{shown(holder.name)}: Cannot create symlink to '{shown(holder.target)}': {gnu.strerror(failure)}")
        state = {...state, failed: true}
      }
    }
  }
  {...state, placeholders: []}
}

# Names of multiply linked files that never met their data-carrying link are
# created empty, linked to a sibling when one exists.
proc create_final_deferred(st: Run, opts: Options, chown_enabled: Bool) [fs, process, env] -> Run {
  var state = st
  for waiting in st.deferred {
    let linked = link_to_inode(state, opts, waiting.name, waiting)
    state = linked.st
    if linked.linked { continue }
    let file = match path_of(waiting.name) {
      Ok(value) => value
      Err(failure) => {
        state = call_error(state, "open", waiting.name, failure)
        continue
      }
    }
    var result = file.write(b"", 384)
    if result is Err(_) and opts.make_dirs {
      let parents = create_parents(state, opts, file)
      state = parents.st
      result = file.write(b"", 384)
    }
    match result {
      Ok(_) => {
        state = set_perms(state, opts, file, waiting.name, waiting.mode, waiting.uid, waiting.gid, waiting.mtime, chown_enabled)
      }
      Err(failure) => { state = call_error(state, "open", waiting.name, failure) }
    }
  }
  {...state, deferred: []}
}

# GNU try_existing_file followed by the per-type creation. Returns the name as
# it should be shown in verbose output (directories lose trailing slashes).
proc extract_entry(st: Run, opts: Options, format: Str, header: Header, content: Bytes, chown_enabled: Bool) [fs, io, error, process, env] -> Named {
  var state = st
  let name = header.name
  let kind = kind_bits(header.mode)
  var existing_dir = false
  if ! opts.to_stdout {
    match path_of(name) {
      Ok(file) => {
        match fs.stat(file, follow_symlinks: false) {
          Ok(info) => {
            if info.kind == "dir" and kind == TYPE_DIR {
              existing_dir = true
            } else if ! opts.unconditional and header.mtime <= info.mtime_ns / 1000000000 {
              gnu.error(f"{shown(name)} not created: newer or same age version exists")
              return {name: name, st: state}
            } else {
              let removed = if info.kind == "dir" { file.remove_dir() } else { file.unlink() }
              match removed {
                Ok(_) => {}
                Err(failure) => {
                  gnu.error(f"cannot remove current {shown(name)}: {gnu.strerror(failure)}")
                  return {name: name, st: state}
                }
              }
            }
          }
          Err(_) => {}
        }
      }
      Err(_) => {}
    }
  }
  if kind == TYPE_FILE {
    state = extract_regular(state, opts, format, header, content, chown_enabled)
  } else if kind == TYPE_DIR {
    return create_directory(state, opts, header, existing_dir, chown_enabled)
  } else if kind == TYPE_CHAR or kind == TYPE_BLOCK or kind == TYPE_FIFO or kind == TYPE_SOCKET {
    state = extract_device(state, opts, header, chown_enabled)
  } else if kind == TYPE_LINK {
    state = extract_link(state, opts, header, content, chown_enabled)
  } else {
    gnu.error(f"{shown(name)}: unknown file type")
  }
  {name: name, st: state}
}

# Reads one line from the controlling terminal (or the batch file) for -r.
proc read_tty_line(descriptor: Int) [process] -> Str? {
  var line: List[Bytes] = []
  while true {
    match unix.read_fd(descriptor, 1) {
      Ok(chunk) => {
        if chunk.is_empty() {
          if line.is_empty() { return null }
          break
        }
        if chunk == b"\n" { break }
        line += [chunk]
      }
      Err(_) => { return null }
    }
  }
  bytes.concat(line).utf8() ?? ""
}

# Runs the list or extract pass over archive bytes already read.
proc copy_in(opts: Options, data: Bytes, patterns: List[Bytes], chown_enabled: Bool, batch_names: List[Str], tty: Int, tty_out: Int) [fs, io, error, process, env, time] -> Result[Bool, CpioError] {
  var st = fresh_run()
  var position = 0
  var format = opts.format
  var consumed = 0
  var peeked_to = 0
  var listing: List[Bytes] = []
  var rename_index = 0
  let now_seconds = time.now() / 1000
  let terminator = if opts.null { b"\0" } else { b"\n" }
  let copy_matching = ! opts.nonmatching
  while true {
    let attempt = read_header(data, position, format, st.warned_reverse)
    let parsed = match attempt {
      Ok(value) => value
      Err(failure) => {
        if ! listing.is_empty() { write_out(bytes.concat(listing)) }
        return Err(failure)
      }
    }
    if format == "" { peeked_to = parsed.at + 512 }
    format = parsed.format
    if parsed.reversed and ! st.warned_reverse { st = {...st, warned_reverse: true} }
    let data_start = parsed.data
    var header = parsed.header
    let size = header.size
    let after = data_start + size + padding(format, size)
    if after > data.len() {
      if ! listing.is_empty() { write_out(bytes.concat(listing)) }
      return Err(CpioError.Fatal(message: "premature end of file"))
    }
    if parsed.usable and header.name == TRAILER {
      consumed = data_start
      break
    }
    position = after
    if ! parsed.usable { continue }
    let safe = safe_name(st, header.name, ! opts.no_abs, false)
    st = safe.st
    header = {...header, name: safe.name}
    let content = data.slice(data_start, size)
    var skip = false
    if ! patterns.is_empty() {
      skip = copy_matching
      for pattern in patterns {
        if skip != copy_matching { break }
        if fnmatch(pattern, header.name) { skip = ! copy_matching }
      }
    }
    let kind = kind_bits(header.mode)
    if skip {
      if header.nlink > 1 and (format == "newc" or format == "crc") and size > 0 {
        var waiting: List[Header] = []
        var adopted: Header? = null
        for candidate in st.deferred {
          if adopted == null and same_inode(candidate, header) {
            adopted = candidate
          } else {
            waiting += [candidate]
          }
        }
        if let target = adopted {
          st = {...st, deferred: waiting}
          st = extract_regular(st, opts, format, {...header, name: target.name}, content, chown_enabled)
        }
      }
    } else if opts.table {
      if opts.verbose {
        var target: Bytes? = null
        if kind == TYPE_LINK { target = content }
        listing += [long_line(header, target, opts.numeric, now_seconds)]
      } else {
        listing += [bytes.concat([header.name, terminator])]
      }
      if opts.only_verify_crc and kind != TYPE_LINK { verify_checksum(format, header, content) }
      if listing.len() >= 512 {
        write_out(bytes.concat(listing))
        listing = []
      }
    } else if opts.only_verify_crc {
      if kind != TYPE_LINK {
        verify_checksum(format, header, content)
        if opts.verbose { eprint f"{shown(header.name)}" }
        if opts.dot { progress(".") }
      }
    } else {
      var entry = header
      var wanted = true
      if opts.rename or opts.rename_batch != null {
        var answer: Str? = null
        if opts.rename {
          let prompt = bytes.concat([b"rename ", header.name, b" -> "])
          let _ = unix.write_fd(tty_out, prompt)
          answer = read_tty_line(tty)
        } else if rename_index < batch_names.len() {
          answer = batch_names[rename_index]
          rename_index += 1
        }
        if let replacement = answer {
          if replacement == "" { wanted = false } else { entry = {...entry, name: bytes.from_text(replacement)} }
        } else {
          wanted = false
        }
      }
      if wanted {
        let extracted = extract_entry(st, opts, format, entry, content, chown_enabled)
        st = extracted.st
        if opts.verbose { eprint f"{shown(extracted.name)}" }
        if opts.dot { progress(".") }
      }
    }
  }
  if ! listing.is_empty() { write_out(bytes.concat(listing)) }
  if opts.dot { progress("\n") }
  st = replace_placeholders(st, opts, chown_enabled)
  st = apply_fixups(st, opts, chown_enabled)
  if format == "newc" or format == "crc" {
    st = create_final_deferred(st, opts, chown_enabled)
  }
  if ! opts.quiet {
    # Input is read in whole blocks, and detecting the format peeks 512 bytes
    # ahead of the first header, so that is how far GNU cpio had read.
    let needed = if consumed > peeked_to { consumed } else { peeked_to }
    let rounded = (needed + opts.block_size - 1) / opts.block_size * opts.block_size
    eprint block_count_text(if rounded > data.len() { data.len() } else { rounded }, opts.block_size)
  }
  Ok(st.failed)
}

# ---------------------------------------------------------------------------
# Copy-out: create an archive from a list of names
# ---------------------------------------------------------------------------

# Archive bytes are collected and written in large pieces; `total` counts every
# byte of the logical stream so the end can be padded to a block.
type Sink = {file: Path?, offset: Int, pending: List[Bytes], pending_size: Int, total: Int}

type Emitted = {sink: Sink, st: Run, written: Bool}

proc sink_flush(sink: Sink) [fs, io, error, process, env] -> Sink {
  if sink.pending.is_empty() { return sink }
  let data = bytes.concat(sink.pending)
  if let target = sink.file {
    match bytes.write_at(target, sink.offset, data, true) {
      Ok(_) => {}
      Err(failure) => {
        gnu.error(f"write error: {gnu.strerror(failure)}")
        exit 2
      }
    }
    return {...sink, offset: sink.offset + data.len(), pending: [], pending_size: 0}
  }
  write_out(data)
  {...sink, pending: [], pending_size: 0}
}

proc sink_emit(sink: Sink, data: Bytes) [fs, io, error, process, env] -> Sink {
  let grown: Sink = {...sink, pending: sink.pending + [data], pending_size: sink.pending_size + data.len(), total: sink.total + data.len()}
  if grown.pending_size >= 4194304 { return sink_flush(grown) }
  grown
}

# One regular file: header, data (zero-padded when the file shrank, cut when it
# grew), and alignment padding.
proc emit_regular(opts: Options, format: Str, sink: Sink, st: Run, header: Header, source: Bytes, atime_ns: Int, mtime_ns: Int) [fs, io, error, process, env] -> Emitted {
  var state = st
  let file = match path_of(source) {
    Ok(value) => value
    Err(failure) => { return {sink: sink, st: call_error(state, "open", source, failure), written: false} }
  }
  let content = match file.read_bytes() {
    Ok(value) => value
    Err(failure) => { return {sink: sink, st: call_error(state, "open", source, failure), written: false} }
  }
  let size = header.size
  var body = content
  if content.len() > size { body = content.slice(0, size) }
  let checksum = if format == "crc" { byte_sum(body) } else { 0 }
  let encoded = encode_header(format, {...header, checksum: checksum}, opts.warn_truncate)
  guard let head = encoded else {
    return {sink: sink, st: state, written: false}
  }
  var out = sink_emit(sink, head)
  out = sink_emit(out, body)
  if content.len() < size {
    let missing = size - content.len()
    gnu.error(f"File {shown(source)} shrunk by {missing} byte{if missing == 1 { "" } else { "s" }}, padding with zeros")
    out = sink_emit(out, bytes.zero(missing) ?? b"")
  } else if content.len() > size {
    let extra = content.len() - size
    gnu.error(f"File {shown(source)} grew, {extra} new byte{if extra == 1 { "" } else { "s" }} not copied")
  }
  let pad = padding(format, size)
  if pad > 0 { out = sink_emit(out, ZERO_PAD.slice(0, pad)) }
  if opts.reset_time {
    match fs.set_times(file, atime_ns: atime_ns, mtime_ns: mtime_ns) {
      Ok(_) => {}
      Err(failure) => { state = call_error(state, "utime", source, failure) }
    }
  }
  {sink: out, st: state, written: true}
}

# Names come from standard input, one per line or NUL-terminated with -0.
proc read_names(opts: Options) [io, error, process, env] -> List[Bytes] {
  match io.stdin_bytes() {
    Ok(data) => split_names(data, if opts.null { 0 } else { 10 })
    Err(failure) => {
      gnu.error(f"read error: {gnu.strerror(failure)}")
      exit 2
    }
  }
}

proc copy_out(opts: Options, format: Str, target: Path?, start: Int, padding_start: Int) [fs, io, error, process, env, time] -> Result[Bool, CpioError] {
  let names = read_names(opts)
  var sink: Sink = {file: target, offset: start, pending: [], pending_size: 0, total: padding_start}
  var st = fresh_run()
  var deferred: List[Deferred] = []
  var translated: Map[Int] = {}
  var next_inode = 0
  for raw in names {
    if raw.is_empty() {
      gnu.error("blank line ignored")
      continue
    }
    let file = match path_of(raw) {
      Ok(value) => value
      Err(failure) => {
        st = call_error(st, "stat", raw, failure)
        continue
      }
    }
    let info = match fs.stat(file, follow_symlinks: opts.dereference) {
      Ok(value) => value
      Err(failure) => {
        st = call_error(st, "stat", raw, failure)
        continue
      }
    }
    let type_bits = type_bits_of(info.kind)
    var header: Header = {
      ino: info.ino,
      mode: type_bits + info.mode.bit_and(4095),
      uid: opts.owner ?? info.uid,
      gid: opts.group ?? info.gid,
      nlink: info.nlink,
      mtime: info.mtime_ns / 1000000000,
      size: info.size,
      dev_major: device_major(info.dev),
      dev_minor: device_minor(info.dev),
      rdev_major: 0,
      rdev_minor: 0,
      checksum: 0,
      name: b"",
    }
    if info.kind == "char" or info.kind == "block" {
      header = {...header, rdev_major: device_major(info.rdev), rdev_minor: device_minor(info.rdev)}
    }
    if opts.renumber {
      if info.nlink > 1 {
        let key = inode_key(header)
        match translated.get(key) {
          Ok(known) => { header = {...header, ino: known} }
          Err(_) => {
            translated = translated.set(key, next_inode)
            header = {...header, ino: next_inode}
            next_inode += 1
          }
        }
      } else {
        header = {...header, ino: next_inode}
        next_inode += 1
      }
    }
    if opts.ignore_devno { header = {...header, dev_major: 0, dev_minor: 0} }
    let safe = safe_name(st, raw, ! opts.no_abs, true)
    st = safe.st
    header = {...header, name: safe.name}
    if type_bits == TYPE_FILE {
      var deferred_here = false
      if (format == "newc" or format == "crc") and header.nlink > 1 {
        var count = 0
        for waiting in deferred {
          if same_inode(waiting.header, header) { count += 1 }
        }
        if header.nlink == count + 1 {
          var remaining: List[Deferred] = []
          for waiting in deferred {
            if same_inode(waiting.header, header) {
              let flat = encode_header(format, {...waiting.header, size: 0}, opts.warn_truncate)
              if let head = flat { sink = sink_emit(sink, head) }
            } else {
              remaining += [waiting]
            }
          }
          deferred = remaining
        } else {
          deferred = [{header: header, source: raw, atime_ns: info.atime_ns, mtime_ns: info.mtime_ns}] + deferred
          deferred_here = true
        }
      }
      if ! deferred_here {
        let emitted = emit_regular(opts, format, sink, st, header, raw, info.atime_ns, info.mtime_ns)
        sink = emitted.sink
        st = emitted.st
        if ! emitted.written { continue }
      }
    } else if type_bits == TYPE_DIR {
      let directory = {...header, size: 0, nlink: if opts.ignore_dirnlink { 2 } else { header.nlink }}
      let encoded = encode_header(format, directory, opts.warn_truncate)
      guard let head = encoded else { continue }
      sink = sink_emit(sink, head)
    } else if type_bits == TYPE_CHAR or type_bits == TYPE_BLOCK or type_bits == TYPE_FIFO or type_bits == TYPE_SOCKET {
      let encoded = encode_header(format, {...header, size: 0}, opts.warn_truncate)
      guard let head = encoded else { continue }
      sink = sink_emit(sink, head)
    } else if type_bits == TYPE_LINK {
      let destination = match file.readlink() {
        Ok(value) => value
        Err(failure) => {
          st = call_error(st, "readlink", raw, failure)
          continue
        }
      }
      let cleaned = safe_name(st, destination.bytes(), ! opts.no_abs, true)
      st = cleaned.st
      let encoded = encode_header(format, {...header, size: cleaned.name.len()}, opts.warn_truncate)
      guard let head = encoded else { continue }
      sink = sink_emit(sink, head)
      sink = sink_emit(sink, cleaned.name)
      let pad = padding(format, cleaned.name.len())
      if pad > 0 { sink = sink_emit(sink, ZERO_PAD.slice(0, pad)) }
    } else {
      gnu.error(f"{shown(raw)}: unknown file type")
    }
    if opts.verbose { eprint f"{shown(raw)}" }
    if opts.dot { progress(".") }
  }
  while ! deferred.is_empty() {
    let waiting = deferred[0]
    var others = 0
    for candidate in deferred {
      if same_inode(candidate.header, waiting.header) { others += 1 }
    }
    deferred = deferred[1..]
    if others == 1 {
      let emitted = emit_regular(opts, format, sink, st, waiting.header, waiting.source, waiting.atime_ns, waiting.mtime_ns)
      sink = emitted.sink
      st = emitted.st
    } else {
      let flat = encode_header(format, {...waiting.header, size: 0}, opts.warn_truncate)
      if let head = flat { sink = sink_emit(sink, head) }
    }
  }
  let trailer: Header = {ino: 0, mode: 0, uid: 0, gid: 0, nlink: 1, mtime: 0, size: 0, dev_major: 0, dev_minor: 0, rdev_major: 0, rdev_minor: 0, checksum: 0, name: TRAILER}
  let closing = encode_header(format, trailer, opts.warn_truncate)
  if let head = closing { sink = sink_emit(sink, head) }
  let fill = (opts.block_size - sink.total % opts.block_size) % opts.block_size
  if fill > 0 { sink = sink_emit(sink, bytes.zero(fill) ?? b"") }
  sink = sink_flush(sink)
  if opts.dot { progress("\n") }
  if ! opts.quiet { eprint block_count_text(sink.total, opts.block_size) }
  Ok(st.failed)
}

# ---------------------------------------------------------------------------
# Copy-pass: copy the listed files into a directory
# ---------------------------------------------------------------------------

proc copy_pass(opts: Options, destination: Bytes, chown_enabled: Bool) [fs, io, error, process, env, time] -> Result[Bool, CpioError] {
  let names = read_names(opts)
  var st = fresh_run()
  for raw in names {
    if raw.is_empty() {
      gnu.error("blank line ignored")
      continue
    }
    if raw == b"." or raw == b"./" { continue }
    let source = match path_of(raw) {
      Ok(value) => value
      Err(failure) => {
        st = call_error(st, "stat", raw, failure)
        continue
      }
    }
    let info = match fs.stat(source, follow_symlinks: opts.dereference) {
      Ok(value) => value
      Err(failure) => {
        st = call_error(st, "stat", raw, failure)
        continue
      }
    }
    var begin = 0
    while raw.byte_at(begin) == 47 { begin += 1 }
    let output = bytes.concat([destination, b"/", raw.slice(begin, raw.len() - begin)])
    let target = match path_of(output) {
      Ok(value) => value
      Err(failure) => {
        st = call_error(st, "open", output, failure)
        continue
      }
    }
    let type_bits = type_bits_of(info.kind)
    let header: Header = {
      ino: info.ino,
      mode: type_bits + info.mode.bit_and(4095),
      uid: info.uid,
      gid: info.gid,
      nlink: info.nlink,
      mtime: info.mtime_ns / 1000000000,
      size: info.size,
      dev_major: device_major(info.dev),
      dev_minor: device_minor(info.dev),
      rdev_major: device_major(info.rdev),
      rdev_minor: device_minor(info.rdev),
      checksum: 0,
      name: output,
    }
    var existing_dir = false
    match fs.stat(target, follow_symlinks: false) {
      Ok(old) => {
        if old.kind == "dir" and info.kind == "dir" {
          existing_dir = true
        } else if ! opts.unconditional and header.mtime <= old.mtime_ns / 1000000000 {
          gnu.error(f"{shown(output)} not created: newer or same age version exists")
          continue
        } else {
          let removed = if old.kind == "dir" { target.remove_dir() } else { target.unlink() }
          match removed {
            Ok(_) => {}
            Err(failure) => {
              gnu.error(f"cannot remove current {shown(output)}: {gnu.strerror(failure)}")
              continue
            }
          }
        }
      }
      Err(_) => {}
    }
    if info.kind == "file" {
      var linked = false
      if opts.link {
        let attempt = link_to_name(st, opts, output, source)
        st = attempt.st
        linked = attempt.linked
      }
      if ! linked and info.nlink > 1 {
        let attempt = link_to_inode(st, opts, output, header)
        st = attempt.st
        linked = attempt.linked
      }
      if ! linked {
        let content = match source.read_bytes() {
          Ok(value) => value
          Err(failure) => {
            st = call_error(st, "open", raw, failure)
            continue
          }
        }
        var written = write_file(target, content, opts.sparse)
        if written is Err(_) and opts.make_dirs {
          let parents = create_parents(st, opts, target)
          st = parents.st
          written = write_file(target, content, opts.sparse)
        }
        match written {
          Ok(_) => {}
          Err(failure) => {
            st = call_error(st, "open", output, failure)
            continue
          }
        }
        st = {...st, copied: st.copied + content.len()}
        st = set_perms(st, opts, target, output, header.mode, header.uid, header.gid, header.mtime, chown_enabled)
        if opts.reset_time {
          for touched in [source, target] {
            match fs.set_times(touched, atime_ns: info.atime_ns, mtime_ns: info.mtime_ns) {
              Ok(_) => {}
              Err(failure) => { st = call_error(st, "utime", raw, failure) }
            }
          }
        }
      }
    } else if info.kind == "dir" {
      let made = create_directory(st, opts, header, existing_dir, chown_enabled)
      st = made.st
    } else if info.kind == "char" or info.kind == "block" or info.kind == "fifo" or info.kind == "socket" {
      var linked = false
      if opts.link {
        let attempt = link_to_name(st, opts, output, source)
        st = attempt.st
        linked = attempt.linked
      }
      if ! linked and info.nlink > 1 {
        let attempt = link_to_inode(st, opts, output, header)
        st = attempt.st
        linked = attempt.linked
      }
      if ! linked {
        st = extract_device(st, {...opts, link: false, to_stdout: false}, {...header, nlink: 1}, chown_enabled)
      }
    } else if info.kind == "symlink" {
      let destination = match source.readlink() {
        Ok(value) => value
        Err(failure) => {
          st = call_error(st, "readlink", raw, failure)
          continue
        }
      }
      st = extract_link(st, {...opts, no_abs: false, to_stdout: false}, header, destination.bytes(), chown_enabled)
    } else {
      gnu.error(f"{shown(raw)}: unknown file type")
    }
    if opts.verbose { eprint f"{shown(output)}" }
    if opts.dot { progress(".") }
  }
  if opts.dot { progress("\n") }
  st = apply_fixups(st, opts, chown_enabled)
  if ! opts.quiet { eprint block_count_text(st.copied, opts.block_size) }
  Ok(st.failed)
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

# A name with a colon before any slash names a remote archive, which needs an
# rsh transport this applet does not have.
pure is_remote(name: Str) -> Bool {
  let colon = name.find(":") ?? -1
  let slash = name.find("/") ?? -1
  colon >= 0 and (slash < 0 or colon < slash)
}

proc archive_path(opts: Options) [fs, process, env] -> Path? {
  guard let name = opts.archive else { return null }
  if is_remote(name) and ! opts.force_local {
    die(f"{name}: remote archives are not supported")
  }
  fp"{name}"
}

# The archive bytes of -i and -t. An empty character device is the end of a
# tape volume to GNU cpio, which then asks a terminal for the next volume.
proc read_archive(opts: Options) [fs, io, error, process, env] -> Bytes {
  var data = b""
  if let source = archive_path(opts) {
    match source.read_bytes() {
      Ok(value) => { data = value }
      Err(failure) => {
        gnu.error(f"Cannot open {source.display()}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  } else {
    match io.stdin_bytes() {
      Ok(value) => { data = value }
      Err(failure) => {
        gnu.error(f"read error: {gnu.strerror(failure)}")
        exit 2
      }
    }
    if data.is_empty() {
      match fs.stat(fp"/dev/stdin") {
        Ok(info) => {
          if info.kind == "char" or info.kind == "block" {
            match unix.open_fd(fp"/dev/tty") {
              Ok(_) => { die("end of volume reached: multi-volume archives are not supported") }
              Err(failure) => { die(f"/dev/tty: {gnu.strerror(failure)}") }
            }
          }
        }
        Err(_) => {}
      }
    }
  }
  data
}

# Pattern operands plus the lines of the -E file.
proc collect_patterns(opts: Options) [fs, error, process, env] -> List[Bytes] {
  var patterns: List[Bytes] = []
  if let name = opts.pattern_file {
    match fp"{name}".read_bytes() {
      Ok(data) => {
        for line in split_names(data, 10) { patterns += [line] }
      }
      Err(failure) => {
        gnu.error(f"{name}: Cannot open: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }
  var result: List[Bytes] = [bytes.from_text(operand) for operand in opts.operands]
  result + patterns
}

# What the mode procedure needs, prepared before the working directory changes.
type Plan = {
  data: Bytes,
  patterns: List[Bytes],
  batch: List[Str],
  tty: Int,
  tty_out: Int,
  target: Path?,
  start: Int,
  pad_start: Int,
  destination: Bytes,
}

# Where -D puts relative archive names: they keep meaning what they meant before.
proc anchored(name: Str, directory: Str?) [fs, error] -> Path {
  if directory == null or name.starts_with("/") { return fp"{name}" }
  match fs.cwd() {
    Ok(here) => fp"{here}/{name}"
    Err(_) => fp"{name}"
  }
}

# Opens or scans the archive of -o: truncates it, or finds where -A appends.
proc prepare_output(opts: Options, format: Str, plan: Plan) [fs, io, error, process, env] -> Plan {
  guard let name = opts.archive else { return plan }
  if is_remote(name) and ! opts.force_local {
    die(f"{name}: remote archives are not supported")
  }
  let location = anchored(name, opts.directory)
  if ! opts.append {
    match location.write(b"") {
      Ok(_) => {}
      Err(failure) => { die(f"Cannot open {name}: {gnu.strerror(failure)}") }
    }
    return {...plan, target: location}
  }
  let existing = match location.read_bytes() {
    Ok(value) => value
    Err(failure) => {
      die(f"Cannot open {name}: {gnu.strerror(failure)}")
      b""
    }
  }
  var position = 0
  var start = 0
  var warned = false
  while true {
    match read_header(existing, position, format, warned) {
      Ok(parsed) => {
        warned = parsed.reversed
        if parsed.usable and parsed.header.name == TRAILER {
          start = parsed.at
          break
        }
        position = parsed.data + parsed.header.size + padding(parsed.format, parsed.header.size)
      }
      Err(CpioError.Fatal {message}) => { die(message) }
    }
  }
  {...plan, target: location, start: start, pad_start: start % opts.block_size}
}

proc prepare(opts: Options) [fs, io, error, process, env] -> Plan {
  var plan: Plan = {data: b"", patterns: [], batch: [], tty: -1, tty_out: -1, target: null, start: 0, pad_start: 0, destination: b""}
  if opts.mode == "in" {
    plan = {...plan, data: read_archive(opts), patterns: collect_patterns(opts)}
    if let name = opts.rename_batch {
      match fp"{name}".read_lines() {
        Ok(lines) => { plan = {...plan, batch: lines} }
        Err(failure) => {
          gnu.error(f"{name}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    } else if opts.rename {
      match unix.open_fd(fp"/dev/tty", nonblock: false) {
        Ok(descriptor) => { plan = {...plan, tty: descriptor} }
        Err(failure) => { die(f"/dev/tty: {gnu.strerror(failure)}") }
      }
      match unix.open_fd(fp"/dev/tty", write: true, nonblock: false) {
        Ok(descriptor) => { plan = {...plan, tty_out: descriptor} }
        Err(failure) => { die(f"/dev/tty: {gnu.strerror(failure)}") }
      }
    }
  } else if opts.mode == "out" {
    plan = prepare_output(opts, opts.format, plan)
  } else {
    let name = opts.operands[0]
    let absolute = if opts.directory != null and ! name.starts_with("/") { anchored(name, opts.directory).display() } else { name }
    plan = {...plan, destination: bytes.from_text(absolute)}
  }
  plan
}

proc execute(opts: Options, plan: Plan, chown_enabled: Bool) [fs, io, error, process, env, time] -> Result[Bool, CpioError] {
  match opts.mode {
    "in" => copy_in(opts, plan.data, plan.patterns, chown_enabled, plan.batch, plan.tty, plan.tty_out)
    "out" => copy_out(opts, opts.format, plan.target, plan.start, plan.pad_start)
    _ => copy_pass(opts, plan.destination, chown_enabled)
  }
}

proc main(...argv: List[Str]) [fs, io, error, process, env, time] {
  let parsed = parse_command_line(argv)
  if parsed.help {
    gnu.help(USAGE)
    return
  }
  if parsed.usage {
    gnu.help(SHORT_USAGE)
    return
  }
  if parsed.version {
    gnu.version("cpio")
    return
  }
  let opts = validate_options(parsed)
  let identity = match unix.id() {
    Ok(value) => value
    Err(failure) => {
      gnu.error(gnu.strerror(failure))
      exit 2
    }
  }
  let chown_enabled = ! opts.no_preserve_owner and (identity.euid == 0 or opts.owner != null or opts.group != null)
  let plan = prepare(opts)
  var outcome: Result[Bool, CpioError] = Ok(false)
  if let directory = opts.directory {
    let wanted = fp"{directory}"
    if opts.make_dirs {
      match fs.stat(wanted) {
        Ok(_) => {}
        Err(_) => {
          match wanted.mkdir(parents: true) {
            Ok(_) => {}
            Err(failure) => { die(f"cannot make directory `{directory}': {gnu.strerror(failure)}") }
          }
        }
      }
    }
    let inside = cd (wanted) { execute(opts, plan, chown_enabled) }
    match inside {
      Ok(result) => { outcome = result }
      Err(failure) => { die(f"cannot change to directory `{directory}': {gnu.strerror(failure)}") }
    }
  } else {
    outcome = execute(opts, plan, chown_enabled)
  }
  match outcome {
    Ok(failed) => {
      if failed { exit 2 }
    }
    Err(failure) => {
      match failure {
        CpioError.Fatal {message} => {
          if message != "" { gnu.error(message) }
        }
      }
      exit 2
    }
  }
}
