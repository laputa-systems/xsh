##! Retains bounded metric samples and projects them into terminal graph cells.
##!
##! Braille cells encode two chronological sample columns; history retention is
##! fixed so resizing changes only the viewport, never the sampling timeline.
use btop_terminal as terminal

## Caps each metric at ten minutes of one-second samples.
export const HISTORY_LIMIT = 600
const MAX_GRAPH_WIDTH = 2000
const MAX_GRAPH_HEIGHT = 2000
const LEFT_DOT_BITS = [1, 2, 4, 64]
const RIGHT_DOT_BITS = [8, 16, 32, 128]

const BRAILLE_GLYPHS: List[Str] = [
  "\u{2800}", "\u{2801}", "\u{2802}", "\u{2803}", "\u{2804}", "\u{2805}", "\u{2806}", "\u{2807}",
  "\u{2808}", "\u{2809}", "\u{280a}", "\u{280b}", "\u{280c}", "\u{280d}", "\u{280e}", "\u{280f}",
  "\u{2810}", "\u{2811}", "\u{2812}", "\u{2813}", "\u{2814}", "\u{2815}", "\u{2816}", "\u{2817}",
  "\u{2818}", "\u{2819}", "\u{281a}", "\u{281b}", "\u{281c}", "\u{281d}", "\u{281e}", "\u{281f}",
  "\u{2820}", "\u{2821}", "\u{2822}", "\u{2823}", "\u{2824}", "\u{2825}", "\u{2826}", "\u{2827}",
  "\u{2828}", "\u{2829}", "\u{282a}", "\u{282b}", "\u{282c}", "\u{282d}", "\u{282e}", "\u{282f}",
  "\u{2830}", "\u{2831}", "\u{2832}", "\u{2833}", "\u{2834}", "\u{2835}", "\u{2836}", "\u{2837}",
  "\u{2838}", "\u{2839}", "\u{283a}", "\u{283b}", "\u{283c}", "\u{283d}", "\u{283e}", "\u{283f}",
  "\u{2840}", "\u{2841}", "\u{2842}", "\u{2843}", "\u{2844}", "\u{2845}", "\u{2846}", "\u{2847}",
  "\u{2848}", "\u{2849}", "\u{284a}", "\u{284b}", "\u{284c}", "\u{284d}", "\u{284e}", "\u{284f}",
  "\u{2850}", "\u{2851}", "\u{2852}", "\u{2853}", "\u{2854}", "\u{2855}", "\u{2856}", "\u{2857}",
  "\u{2858}", "\u{2859}", "\u{285a}", "\u{285b}", "\u{285c}", "\u{285d}", "\u{285e}", "\u{285f}",
  "\u{2860}", "\u{2861}", "\u{2862}", "\u{2863}", "\u{2864}", "\u{2865}", "\u{2866}", "\u{2867}",
  "\u{2868}", "\u{2869}", "\u{286a}", "\u{286b}", "\u{286c}", "\u{286d}", "\u{286e}", "\u{286f}",
  "\u{2870}", "\u{2871}", "\u{2872}", "\u{2873}", "\u{2874}", "\u{2875}", "\u{2876}", "\u{2877}",
  "\u{2878}", "\u{2879}", "\u{287a}", "\u{287b}", "\u{287c}", "\u{287d}", "\u{287e}", "\u{287f}",
  "\u{2880}", "\u{2881}", "\u{2882}", "\u{2883}", "\u{2884}", "\u{2885}", "\u{2886}", "\u{2887}",
  "\u{2888}", "\u{2889}", "\u{288a}", "\u{288b}", "\u{288c}", "\u{288d}", "\u{288e}", "\u{288f}",
  "\u{2890}", "\u{2891}", "\u{2892}", "\u{2893}", "\u{2894}", "\u{2895}", "\u{2896}", "\u{2897}",
  "\u{2898}", "\u{2899}", "\u{289a}", "\u{289b}", "\u{289c}", "\u{289d}", "\u{289e}", "\u{289f}",
  "\u{28a0}", "\u{28a1}", "\u{28a2}", "\u{28a3}", "\u{28a4}", "\u{28a5}", "\u{28a6}", "\u{28a7}",
  "\u{28a8}", "\u{28a9}", "\u{28aa}", "\u{28ab}", "\u{28ac}", "\u{28ad}", "\u{28ae}", "\u{28af}",
  "\u{28b0}", "\u{28b1}", "\u{28b2}", "\u{28b3}", "\u{28b4}", "\u{28b5}", "\u{28b6}", "\u{28b7}",
  "\u{28b8}", "\u{28b9}", "\u{28ba}", "\u{28bb}", "\u{28bc}", "\u{28bd}", "\u{28be}", "\u{28bf}",
  "\u{28c0}", "\u{28c1}", "\u{28c2}", "\u{28c3}", "\u{28c4}", "\u{28c5}", "\u{28c6}", "\u{28c7}",
  "\u{28c8}", "\u{28c9}", "\u{28ca}", "\u{28cb}", "\u{28cc}", "\u{28cd}", "\u{28ce}", "\u{28cf}",
  "\u{28d0}", "\u{28d1}", "\u{28d2}", "\u{28d3}", "\u{28d4}", "\u{28d5}", "\u{28d6}", "\u{28d7}",
  "\u{28d8}", "\u{28d9}", "\u{28da}", "\u{28db}", "\u{28dc}", "\u{28dd}", "\u{28de}", "\u{28df}",
  "\u{28e0}", "\u{28e1}", "\u{28e2}", "\u{28e3}", "\u{28e4}", "\u{28e5}", "\u{28e6}", "\u{28e7}",
  "\u{28e8}", "\u{28e9}", "\u{28ea}", "\u{28eb}", "\u{28ec}", "\u{28ed}", "\u{28ee}", "\u{28ef}",
  "\u{28f0}", "\u{28f1}", "\u{28f2}", "\u{28f3}", "\u{28f4}", "\u{28f5}", "\u{28f6}", "\u{28f7}",
  "\u{28f8}", "\u{28f9}", "\u{28fa}", "\u{28fb}", "\u{28fc}", "\u{28fd}", "\u{28fe}", "\u{28ff}"
]

## Distinguishes observed zeroes from intervals without a graph value.
export enum SampleState: Str {
  Observed = "observed",
  Gap = "gap",
  Unavailable = "unavailable",
}

## Names the automatically selected graph glyph family.
export enum SymbolSet: Str {
  Braille = "braille",
  Tty = "tty",
}

## Retains one value or an explicit gap at its monotonic sample time.
export type Sample = {at_ns: Int, value: Float?, state: SampleState}
## Holds the bounded source-time sequence, independent of viewport geometry.
export type History = {samples: List[Sample]}
## Contains the visible projection and the time columns that produced it.
export type Frame = {
  width: Int,
  height: Int,
  symbol_set: SymbolSet,
  minimum: Float,
  scale: Float,
  rows: List[Str],
  visible_sample_times: List[Int],
  visible_states: List[SampleState],
}

## Starts with no retained graph samples.
export pure empty_history() -> History {
  {samples: []}
}

## Adds one ordered observation and evicts only samples beyond the fixed horizon.
## Replay timestamps may start at zero because they use a virtual time origin.
export pure append(history: History, sample: Sample) -> Result[History, Error] {
  return Err(error.failure("graph sample time cannot be negative")) when sample.at_ns < 0
  if history.samples.len() > 0 and sample.at_ns <= history.samples[history.samples.len() - 1].at_ns {
    return Err(error.failure("graph sample times must increase"))
  }
  if sample.state == .Observed and sample.value == null {
    return Err(error.failure("observed graph samples require a value"))
  }
  if sample.state != .Observed and sample.value != null {
    return Err(error.failure("missing graph samples cannot carry a value"))
  }
  if let value = sample.value {
    return Err(error.failure("graph sample value must be finite")) when value != value
  }

  let samples = history.samples + [sample]
  Ok({samples: if samples.len() > HISTORY_LIMIT { samples |> drop(1) } else { samples }})
}

## Resolves the single in-scope graph symbol from captured terminal capabilities.
export pure symbol_set(capabilities: terminal.Capabilities) -> SymbolSet {
  if capabilities.utf8 { .Braille } else { .Tty }
}

## Maps one complete Braille dot mask to its Unicode scalar.
export pure braille_glyph(mask: Int) -> Result[Str, Error] {
  if mask < 0 or mask >= BRAILLE_GLYPHS.len() {
    return Err(error.failure("Braille dot mask must fit eight dots"))
  }
  Ok(BRAILLE_GLYPHS[mask])
}

pure retained_scale(history: History, minimum: Float) -> Float {
  var maximum = minimum + 1.0
  for sample in history.samples {
    if sample.state == .Observed {
      if let value = sample.value {
        if value > maximum {
          maximum = value
        }
      }
    }
  }
  maximum
}

pure bounded_ratio(value: Float, minimum: Float, maximum: Float) -> Float {
  return 0.0 when value <= minimum
  return 1.0 when value >= maximum
  (value - minimum) / (maximum - minimum)
}

pure sample_pixels(sample: Sample, minimum: Float, maximum: Float, rows: Int) -> Result[Int, Error] {
  return Ok(0) when sample.state != .Observed or sample.value == null
  let ratio = bounded_ratio(sample.value ?? minimum, minimum, maximum)
  Ok((ratio * (rows * 4).float()).round()?)
}

pure dots_in_cell_row(pixels: Int, row: Int, rows: Int, inverted: Bool, right: Bool) -> Int {
  let vertical_index = if inverted { row } else { rows - row - 1 }
  let unclamped = pixels - vertical_index * 4
  let count = if unclamped < 0 { 0 } else if unclamped > 4 { 4 } else { unclamped }
  let bits = if right { RIGHT_DOT_BITS } else { LEFT_DOT_BITS }
  var mask = 0
  for dot in range(4) {
    let filled = if inverted { dot < count } else { dot >= 4 - count }
    if filled {
      mask += bits[dot]
    }
  }
  mask
}

pure tty_glyph(sample: Sample, minimum: Float, maximum: Float, row: Int, height: Int, inverted: Bool) -> Result[Str, Error] {
  return Ok(" ") when sample.state != .Observed or sample.value == null
  let pixels = sample_pixels(sample, minimum, maximum, height)?
  let filled_rows = (pixels + 3) / 4
  let filled = if inverted { row < filled_rows } else { row >= height - filled_rows }
  return Ok(" ") unless filled
  Ok(if filled_rows <= 1 { "." } else if filled_rows < height { ":" } else { "#" })
}

pure check_history(history: History) -> Result[Unit, Error] {
  if history.samples.len() > HISTORY_LIMIT {
    return Err(error.failure("graph history exceeds its retained sample bound"))
  }
  var previous_at: Int? = null
  for sample in history.samples {
    if sample.at_ns < 0 or (previous_at != null and sample.at_ns <= (previous_at ?? sample.at_ns)) {
      return Err(error.failure("graph history times must increase"))
    }
    if sample.state == .Observed and sample.value == null {
      return Err(error.failure("observed graph samples require a value"))
    }
    if sample.state != .Observed and sample.value != null {
      return Err(error.failure("missing graph samples cannot carry a value"))
    }
    if let value = sample.value {
      return Err(error.failure("graph sample value must be finite")) when value != value
    }
    previous_at = sample.at_ns
  }
  Ok()
}

## Projects retained samples without changing history or deriving new samples.
export pure project(
  history: History,
  capabilities: terminal.Capabilities,
  width: Int,
  height: Int,
  inverted: Bool,
  maximum: Float?,
  minimum: Float = 0.0,
) -> Result[Frame, Error] {
  if width <= 0 or width > MAX_GRAPH_WIDTH or height <= 0 or height > MAX_GRAPH_HEIGHT {
    return Err(error.failure("graph dimensions are outside their bounds"))
  }
  let _ = check_history(history)?
  let scale = if let requested = maximum { requested } else { retained_scale(history, minimum) }
  return Err(error.failure("graph maximum must exceed its minimum")) when scale <= minimum

  let symbols = symbol_set(capabilities)
  let samples_per_cell = if symbols == .Braille { 2 } else { 1 }
  let capacity = width * samples_per_cell
  let start = if history.samples.len() > capacity { history.samples.len() - capacity } else { 0 }
  let visible = history.samples |> drop(start)
  let times: List[Int] = [sample.at_ns for sample in visible]
  let states: List[SampleState] = [sample.state for sample in visible]
  let graph_width = if symbols == .Braille { (visible.len() + 1) / 2 } else { visible.len() }
  let left_padding = width - graph_width

  let rows: List[Str] = collect {
    for row in range(height) {
      var line = ""
      for column in range(width) {
        if column < left_padding {
          line += " "
          continue
        }
        if symbols == .Braille {
          let distance_from_right = width - column - 1
          let right_index = visible.len() - 1 - distance_from_right * 2
          if right_index < 0 {
            line += "\u{2800}"
            continue
          }
          let right_sample = visible[right_index]
          let right_pixels = sample_pixels(right_sample, minimum, scale, height)?
          let right_mask = dots_in_cell_row(right_pixels, row, height, inverted, true)
          let left_index = right_index - 1
          let left_mask = if left_index >= 0 {
            let left_pixels = sample_pixels(visible[left_index], minimum, scale, height)?
            dots_in_cell_row(left_pixels, row, height, inverted, false)
          } else {
            0
          }
          line += braille_glyph(left_mask + right_mask)?
        } else {
          let visible_index = visible.len() - width + column
          if visible_index < 0 {
            line += " "
            continue
          }
          line += tty_glyph(visible[visible_index], minimum, scale, row, height, inverted)?
        }
      }
      yield line
    }
  }

  Ok({
    width: width,
    height: height,
    symbol_set: symbols,
    minimum: minimum,
    scale: scale,
    rows: rows,
    visible_sample_times: times,
    visible_states: states,
  })
}
