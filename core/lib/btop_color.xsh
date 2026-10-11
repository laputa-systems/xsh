##! Maps the fixed monitor palette to the captured terminal color depth.
##!
##! Palette roles use one truecolor source so reduced-depth output stays
##! deterministic across environments and honors NO_COLOR centrally.

use btop_terminal as terminal

## Names the fixed semantic color roles used by monitor rows.
export enum Role: Str {
  Cpu = "cpu",
  Memory = "memory",
  Network = "network",
  Process = "process",
}

## Stores one palette entry in truecolor channels.
export type Rgb = {red: Int, green: Int, blue: Int}

## Returns the one fixed truecolor value assigned to a panel role.
export pure truecolor(role: Role) -> Rgb {
  match role {
    Cpu => {red: 0, green: 255, blue: 0}
    Memory => {red: 0, green: 128, blue: 255}
    Network => {red: 255, green: 170, blue: 0}
    Process => {red: 192, green: 192, blue: 192}
  }
}

pure cube_level(value: Int) -> Int {
  (value * 5 + 127) / 255
}

pure cube_component(level: Int) -> Int {
  if level == 0 { 0 } else { 55 + level * 40 }
}

pure square(value: Int) -> Int { value * value }

pure distance(rgb: Rgb, red: Int, green: Int, blue: Int) -> Int {
  square(rgb.red - red) + square(rgb.green - green) + square(rgb.blue - blue)
}

pure xterm256(rgb: Rgb) -> Int {
  let red = cube_level(rgb.red)
  let green = cube_level(rgb.green)
  let blue = cube_level(rgb.blue)
  let cube = 16 + 36 * red + 6 * green + blue
  let gray = ((rgb.red + rgb.green + rgb.blue) / 3 - 8 + 5) / 10
  let gray_level = if gray < 0 { 0 } else if gray > 23 { 23 } else { gray }
  let gray_component = 8 + gray_level * 10
  let cube_distance = distance(rgb, cube_component(red), cube_component(green), cube_component(blue))
  let gray_distance = distance(rgb, gray_component, gray_component, gray_component)
  if gray_distance < cube_distance { 232 + gray_level } else { cube }
}

const ANSI16: List[Rgb] = [
  {red: 0, green: 0, blue: 0},
  {red: 128, green: 0, blue: 0},
  {red: 0, green: 128, blue: 0},
  {red: 128, green: 128, blue: 0},
  {red: 0, green: 0, blue: 128},
  {red: 128, green: 0, blue: 128},
  {red: 0, green: 128, blue: 128},
  {red: 192, green: 192, blue: 192},
  {red: 128, green: 128, blue: 128},
  {red: 255, green: 0, blue: 0},
  {red: 0, green: 255, blue: 0},
  {red: 255, green: 255, blue: 0},
  {red: 0, green: 0, blue: 255},
  {red: 255, green: 0, blue: 255},
  {red: 0, green: 255, blue: 255},
  {red: 255, green: 255, blue: 255},
]

pure ansi16(rgb: Rgb) -> Int {
  var selected = 0
  var selected_distance = 9223372036854775807
  for index in range(ANSI16.len()) {
    let candidate = ANSI16[index]
    let candidate_distance = distance(rgb, candidate.red, candidate.green, candidate.blue)
    if candidate_distance < selected_distance {
      selected = index
      selected_distance = candidate_distance
    }
  }
  selected
}

## Converts one role to a foreground SGR sequence at the detected depth.
export pure foreground(role: Role, depth: terminal.ColorDepth) -> Str {
  let rgb = truecolor(role)
  match depth {
    terminal.NoColor => ""
    terminal.TrueColor => f"\u{1b}[38;2;{rgb.red};{rgb.green};{rgb.blue}m"
    terminal.Ansi256 => f"\u{1b}[38;5;{xterm256(rgb)}m"
    terminal.Ansi16 => {
      let index = ansi16(rgb)
      if index < 8 { f"\u{1b}[{30 + index}m" } else { f"\u{1b}[{90 + index - 8}m" }
    }
  }
}
