cli main(output: Path) {
  output.write(bytes.concat([b"a" for _ in range(131072)]))?
}
