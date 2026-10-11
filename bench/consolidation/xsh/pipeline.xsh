cli main(root: Path) {
  const define = rx"^#define[ \t]+(CAP_[A-Z0-9_]+)[ \t]+([0-9]+)[ \t]*$"
  let names = collect {
    for line in fp"{root}/capability.h".lines()? {
      if let [_, cap, value] = define.captures(line) {
        yield "{\"" + cap.lower() + "\"," + value + "},"
      }
    }
  }
  print names.join("\n")
}
