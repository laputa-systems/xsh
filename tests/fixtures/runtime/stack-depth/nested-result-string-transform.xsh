proc transform(attr: Str) [error] -> Result[Str] {
  return f"${attr.replace("input_prop", "INPUT_PROP")
    .replace("mt_tool", "MT_TOOL")
    .replace("ev", "EV")
    .replace("rel", "REL")
    .replace("abs", "ABS")
    .replace("key", "KEY")
    .replace("btn", "BTN")
    .replace("led", "LED")
    .replace("snd", "SND")
    .replace("msc", "MSC")
    .replace("sw", "SW")
    .replace("ff", "FF")
    .replace("syn", "SYN")
    .replace("rep", "REP")}_MAX"
}

proc main() [error] -> Result[Unit] {
  print transform("mt_tool")?
}
