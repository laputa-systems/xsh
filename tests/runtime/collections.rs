use super::common::*;

#[test]
fn fs_walk_streams_lazily_and_short_circuits_take_first_any_and_break() {
    // A flat directory of 50 files. A lazy `fs.walk` must stop pulling entries
    // as soon as the consumer is satisfied, so a `take(3)`/`first`/`any`/`break`
    // touches only a handful of entries rather than the whole tree.
    let root = temp_path("fs-walk-lazy-root");
    std::fs::create_dir_all(&root).expect("create lazy walk root");
    for index in 0..50 {
        std::fs::write(root.join(format!("f{index:02}.txt")), "x").expect("write file");
    }
    let source = format!(
        "\
let root = Path({})

var pulled = 0
let first3 = fs.walk(root)
|> tee {{ |entry| pulled = pulled + 1 }}
|> where .kind == \"file\"
|> take(3)
|> map .name
print f\"take ok=${{pulled < 50 && first3.len() == 3}}\"

var pulled_any = 0
let any_file = fs.walk(root)
|> tee {{ |entry| pulled_any = pulled_any + 1 }}
|> any .kind == \"file\"
print f\"any ok=${{pulled_any < 50 && any_file}}\"

var pulled_break = 0
for entry in fs.walk(root) |> where .kind == \"file\" {{
  pulled_break = pulled_break + 1
  if pulled_break >= 2 {{ break }}
}}
print f\"break seen=${{pulled_break}}\"

let total = fs.walk(root) |> where .kind == \"file\" |> count()
print f\"total=${{total}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-walk-lazy", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "take ok=true\n\
any ok=true\n\
break seen=2\n\
total=50\n"
    );
    let _ = std::fs::remove_dir_all(root);
}
