##! Embedded implementation of the public `bytes` module.
# Internal implementation module. The public contract stays in the standard API
# registry; only the presentation algorithm lives here.

## Format a byte count with binary unit suffixes.
##
## A negative size renders as a lone `-`, preserving the baseline's refusal to
## guess a magnitude. The magnitude advances through binary units while it is at
## least 1024 and a coarser unit remains, then renders with no decimal below
## 1024 and one decimal below ten of the chosen unit. The decimal formatting is
## the shared numeric kernel, not a reimplementation.
export pure human(size: Int) -> Str {
  if size < 0 {
    return "-"
  }
  let units = ["", "K", "M", "G", "T", "P", "E"]
  var amount = size.float()
  var unit_index = 0
  while amount >= 1024.0 and unit_index + 1 < units.len() {
    amount = amount / 1024.0
    unit_index = unit_index + 1
  }
  let unit = units.get(unit_index, "")
  if unit == "" {
    return amount.format(0)
  }
  if amount < 10.0 {
    return amount.format(1) + unit
  }
  return amount.format(0) + unit
}
