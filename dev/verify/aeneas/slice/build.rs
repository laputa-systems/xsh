const NODE: &str = "../../../../src/syntax/node.rs";
const COPY: &str = "src/binary_op.rs";
const DECLARATION: &str = "pub enum BinaryOp {";

fn binary_op_block(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|line| *line == DECLARATION)
        .unwrap_or_else(|| panic!("{NODE} has no `{DECLARATION}` line"));
    let mut first = start;
    while first > 0 && lines[first - 1].starts_with("#[") {
        first -= 1;
    }
    assert!(first < start, "{NODE}: BinaryOp has no derive attribute");
    let end = start
        + lines[start..]
            .iter()
            .position(|line| *line == "}")
            .unwrap_or_else(|| panic!("{NODE}: BinaryOp has no closing brace"));
    let mut block = lines[first..=end].join("\n");
    block.push('\n');
    block
}

fn main() {
    println!("cargo:rerun-if-changed={NODE}");
    println!("cargo:rerun-if-changed={COPY}");
    let node = std::fs::read_to_string(NODE).unwrap_or_else(|error| panic!("{NODE}: {error}"));
    let copy = std::fs::read_to_string(COPY).unwrap_or_else(|error| panic!("{COPY}: {error}"));
    assert!(
        copy == binary_op_block(&node),
        "{COPY} is not the verbatim BinaryOp declaration of {NODE}"
    );
}
