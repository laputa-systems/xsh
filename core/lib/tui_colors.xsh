##! Parses `LS_COLORS` and renders semantic cell styles as ANSI SGR.
##!
##! Only the keys an index can answer are used: `di` for directories, `ln` for
##! symlinks, `fi` for regular files, and `*` name patterns. Permission-based
##! keys are parsed but ignored because a name index does not retain mode bits.

## Names one terminal color from the basic, bright, indexed, or RGB ranges.
export enum Color {
  Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
  BrightBlack, BrightRed, BrightGreen, BrightYellow,
  BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
  Ansi256(Int), Rgb(Int, Int, Int),
}

## Carries the attributes one clear region of text is drawn with.
export type Style = {
  fg: Color?,
  bg: Color?,
  bold: Bool,
  dim: Bool,
  reversed: Bool,
}

## Describes the entry kinds a name index can distinguish.
export enum EntryKind { Directory, Symlink, Regular, Other }

## Pairs one `*` pattern with the style it selects.
export type PatternColor = {pattern: Bytes, style: Style}

## Retains default type colors and name patterns in `LS_COLORS` order.
export type LsColors = {dir: Style, file: Style, symlink: Style, patterns: List[PatternColor]}

## The style with no attributes set.
export const PLAIN: Style = {fg: null, bg: null, bold: false, dim: false, reversed: false}
## The bold style without a color.
export const BOLD: Style = {fg: null, bg: null, bold: true, dim: false, reversed: false}
## The dim style without a color.
export const DIM: Style = {fg: null, bg: null, bold: false, dim: true, reversed: false}
## The reversed style without a color.
export const REVERSED: Style = {fg: null, bg: null, bold: false, dim: false, reversed: true}
## The yellow foreground used for incomplete or stale entries.
export const YELLOW: Style = {fg: Yellow, bg: null, bold: false, dim: false, reversed: false}
## The red foreground used for marked and excluded entries.
export const RED: Style = {fg: Red, bg: null, bold: false, dim: false, reversed: false}

## Returns the built-in directory, file, and symlink colors.
export pure default_ls_colors() -> LsColors {
  {
    dir: {fg: Blue, bg: null, bold: true, dim: false, reversed: false},
    file: PLAIN,
    symlink: {fg: Cyan, bg: null, bold: true, dim: false, reversed: false},
    patterns: [],
  }
}

## Parses colon-separated `LS_COLORS`; missing or malformed entries keep defaults.
export pure parse_ls_colors(value: Str) -> LsColors {
  var colors = default_ls_colors()
  for entry in value.split(":") {
    let separator = entry.find("=")
    continue when separator == null
    let at = separator ?? 0
    let key = entry[..at]
    continue when key == ""
    let style = parse_sgr(entry[at + 1..])
    continue when style == null
    let found = style ?? PLAIN
    if key.starts_with("*") {
      colors = {...colors, patterns: colors.patterns + [{pattern: bytes.from_text(key), style: found}]}
    } else if key == "di" {
      colors = {...colors, dir: found}
    } else if key == "fi" {
      colors = {...colors, file: found}
    } else if key == "ln" {
      colors = {...colors, symlink: found}
    }
  }
  colors
}

## Selects a row's base style: type first, then name patterns, then the file color.
export pure style_for(colors: LsColors, name: Bytes, kind: EntryKind) -> Style {
  match kind {
    Directory => colors.dir
    Symlink => colors.symlink
    Regular | Other => {
      for candidate in colors.patterns {
        return candidate.style when glob_match(candidate.pattern, name)
      }
      colors.file
    }
  }
}

## Adds the cursor highlight while keeping foreground and background colors.
export pure with_cursor(style: Style, focused: Bool) -> Style {
  if focused {
    {...style, reversed: true}
  } else {
    {...style, bold: true}
  }
}

## Renders one style as a complete SGR sequence beginning with a reset.
export pure ansi(style: Style) -> Str {
  var output = "\u{1b}[0"
  if style.bold { output += ";1" }
  if style.dim { output += ";2" }
  if style.reversed { output += ";7" }
  if let color = style.fg {
    output += foreground_code(color, true)
  }
  if let color = style.bg {
    output += foreground_code(color, false)
  }
  f"{output}m"
}

pure foreground_code(color: Color, foreground: Bool) -> Str {
  match color {
    Black => if foreground { ";30" } else { ";40" }
    Red => if foreground { ";31" } else { ";41" }
    Green => if foreground { ";32" } else { ";42" }
    Yellow => if foreground { ";33" } else { ";43" }
    Blue => if foreground { ";34" } else { ";44" }
    Magenta => if foreground { ";35" } else { ";45" }
    Cyan => if foreground { ";36" } else { ";46" }
    White => if foreground { ";37" } else { ";47" }
    BrightBlack => if foreground { ";90" } else { ";100" }
    BrightRed => if foreground { ";91" } else { ";101" }
    BrightGreen => if foreground { ";92" } else { ";102" }
    BrightYellow => if foreground { ";93" } else { ";103" }
    BrightBlue => if foreground { ";94" } else { ";104" }
    BrightMagenta => if foreground { ";95" } else { ";105" }
    BrightCyan => if foreground { ";96" } else { ";106" }
    BrightWhite => if foreground { ";97" } else { ";107" }
    Ansi256(index) => if foreground { f";38;5;{index}" } else { f";48;5;{index}" }
    Rgb(red, green, blue) => if foreground { f";38;2;{red};{green};{blue}" } else { f";48;2;{red};{green};{blue}" }
  }
}

pure parse_sgr(value: Str) -> Style? {
  if value == "" or value == "target" {
    return null
  }
  var style = PLAIN
  var seen = false
  let parts = value.split(";")
  var index = 0
  while index < parts.len() {
    let code = parts[index].trim()
    index += 1
    var number = 0
    if let Ok(parsed) = code.parse_int() {
      number = parsed
    } else {
      continue
    }
    seen = true
    if number == 0 {
      style = PLAIN
    } else if number == 1 {
      style = {...style, bold: true}
    } else if number == 2 {
      style = {...style, dim: true}
    } else if number == 7 {
      style = {...style, reversed: true}
    } else if number == 22 {
      style = {...style, bold: false, dim: false}
    } else if number == 27 {
      style = {...style, reversed: false}
    } else if number >= 30 and number <= 37 {
      style = {...style, fg: basic(number - 30)}
    } else if number >= 90 and number <= 97 {
      style = {...style, fg: bright(number - 90)}
    } else if number == 39 {
      style = {...style, fg: null}
    } else if number >= 40 and number <= 47 {
      style = {...style, bg: basic(number - 40)}
    } else if number >= 100 and number <= 107 {
      style = {...style, bg: bright(number - 100)}
    } else if number == 49 {
      style = {...style, bg: null}
    } else if number == 38 or number == 48 {
      let foreground = number == 38
      if index < parts.len() {
        if let Ok(mode) = parts[index].trim().parse_int() {
          index += 1
          if mode == 5 and index < parts.len() {
            if let Ok(level) = parts[index].trim().parse_int() {
              index += 1
              if level >= 0 and level <= 255 {
                style = if foreground { {...style, fg: Ansi256(level)} } else { {...style, bg: Ansi256(level)} }
              }
            }
          } else if mode == 2 and index + 2 < parts.len() {
            let red = parts[index].trim().parse_int()
            let green = parts[index + 1].trim().parse_int()
            let blue = parts[index + 2].trim().parse_int()
            if let Ok(parsed_red) = red {
              if let Ok(parsed_green) = green {
                if let Ok(parsed_blue) = blue {
                  if parsed_red >= 0 and parsed_red <= 255 and parsed_green >= 0 and parsed_green <= 255 and parsed_blue >= 0 and parsed_blue <= 255 {
                    index += 3
                    style = if foreground {
                      {...style, fg: Rgb(parsed_red, parsed_green, parsed_blue)}
                    } else {
                      {...style, bg: Rgb(parsed_red, parsed_green, parsed_blue)}
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  if seen { style } else { null }
}

pure basic(index: Int) -> Color {
  match index {
    0 => Black
    1 => Red
    2 => Green
    3 => Yellow
    4 => Blue
    5 => Magenta
    6 => Cyan
    else => White
  }
}

pure bright(index: Int) -> Color {
  match index {
    0 => BrightBlack
    1 => BrightRed
    2 => BrightGreen
    3 => BrightYellow
    4 => BrightBlue
    5 => BrightMagenta
    6 => BrightCyan
    else => BrightWhite
  }
}

pure glob_match(pattern: Bytes, name: Bytes) -> Bool {
  var px = 0
  var nx = 0
  var star: Int? = null
  var mark = 0
  while nx < name.len() {
    if px < pattern.len() and (pattern.byte_at(px) ?? 0) == 42 {
      star = px
      mark = nx
      px += 1
    } else if px < pattern.len() and (pattern.byte_at(px) ?? 0) == (name.byte_at(nx) ?? 0) {
      px += 1
      nx += 1
    } else if let star_pos = star {
      mark += 1
      nx = mark
      px = star_pos + 1
    } else {
      return false
    }
  }
  while px < pattern.len() and (pattern.byte_at(px) ?? 0) == 42 {
    px += 1
  }
  px == pattern.len()
}
