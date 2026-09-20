##! Embedded implementation of the public `time` module.
# Internal implementation module. The public contract stays in the standard API
# registry; only the compact-duration rendering lives here.

## Render a duration in seconds as a compact fixed-width field.
##
## Negative durations clamp to zero. Days and hours select the widest unit that
## applies, and the baseline pads the hours form with two leading spaces so the
## three renderings share one column width.
export pure duration_compact(seconds: Int) -> Str {
  var rest = seconds
  if rest < 0 {
    rest = 0
  }
  let ss = rest % 60
  rest = rest / 60
  let mm = rest % 60
  rest = rest / 60
  let hh = rest % 24
  let dd = rest / 24

  if dd > 0 {
    return f"${dd:>3}d${hh:02}h"
  }
  if hh > 0 {
    return f"  ${hh:>2}h${mm:02}m"
  }
  return f"   ${mm:>2}:${ss:02}"
}
