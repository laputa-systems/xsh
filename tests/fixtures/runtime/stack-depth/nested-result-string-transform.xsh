proc transform(attr: Str) [error] -> Result[Str] {
  return f"{attr.replace("input_prop", with: "INPUT_PROP").replace("mt_tool", with: "MT_TOOL").replace("ev", with: "EV").replace("rel", with: "REL").replace("abs", with: "ABS").replace("key", with: "KEY").replace("btn", with: "BTN").replace("led", with: "LED").replace("snd", with: "SND").replace("msc", with: "MSC").replace("sw", with: "SW").replace("ff", with: "FF").replace("syn", with: "SYN").replace("rep", with: "REP")}_MAX"
}

proc main() [error] -> Result[Unit] {
  print transform("mt_tool")?
}
