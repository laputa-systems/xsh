//! Metamorphic checks for inference: a transformation that preserves meaning
//! must preserve acceptance and output.
//!
//! Two families run at fixed seeds. Generated programs from the soundness
//! generator are reordered, renamed and given explicit return types; the
//! reference evaluator's prediction must still hold for every variant.
//! Hand-built generic programs are renamed, reordered, called with named
//! arguments and rewritten through an extracted helper; their output must not
//! change. A control transformation that changes meaning must be detected, so
//! a comparison that cannot fail does not pass.

use std::path::PathBuf;
use xsh_fuzz::ast::FnKind;
use xsh_fuzz::generator::{GenConfig, Generated, generate};
use xsh_fuzz::harness::{Sandbox, check_text};

const GENERATED_SEEDS: u64 = 40;

fn sandbox() -> Sandbox {
    Sandbox::new(PathBuf::from(test_bin!("xsh-fuzz")))
}

fn run(context: &str, source: &str) -> String {
    let report = check_text("metamorphic.xsh", source);
    assert!(report.accepted(), "{context}: {report:?}\n{source}");
    let run = sandbox().run(source).expect("run the variant");
    assert_eq!(run.status, Some(0), "{context}: {run:?}\n{source}");
    run.stdout
}

fn rename_identifiers(source: &str) -> String {
    const PREFIXES: [&str; 6] = ["fn", "v", "w", "i", "m", "p"];
    let mut out = String::with_capacity(source.len() + source.len() / 8);
    let mut chars = source.char_indices().peekable();
    while let Some((start, character)) = chars.next() {
        if !(character.is_ascii_alphabetic() || character == '_') {
            out.push(character);
            continue;
        }
        let mut end = start + character.len_utf8();
        while let Some(&(next, following)) = chars.peek() {
            if following.is_alphanumeric() || following == '_' {
                end = next + following.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        let word = &source[start..end];
        let generated = PREFIXES.iter().any(|prefix| {
            word.strip_prefix(prefix).is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        });
        out.push_str(word);
        if generated {
            out.push_str("_renamed");
        }
    }
    out
}

fn reverse_functions(source: &str) -> String {
    let mut items: Vec<(bool, String)> = Vec::new();
    for line in source.split_inclusive('\n') {
        let function = ["pure ", "proc ", "stream "].iter().any(|keyword| line.starts_with(keyword));
        let declaration = function || ["error ", "const ", "type ", "enum "].iter().any(|keyword| line.starts_with(keyword));
        if declaration || items.is_empty() {
            items.push((function, line.to_owned()));
        } else {
            items.last_mut().expect("an item is open").1.push_str(line);
        }
    }
    let mut functions: Vec<String> = items.iter().filter(|(function, _)| *function).map(|(_, text)| text.clone()).collect();
    functions.reverse();
    let mut functions = functions.into_iter();
    items.into_iter().map(|(function, text)| if function { functions.next().expect("a function remains") } else { text }).collect()
}

fn assert_generated_variant(generated: &Generated, label: &str, source: &str) {
    let output = run(&format!("seed {}: {label}", generated.seed), source);
    assert_eq!(output, generated.expected, "seed {}: {label}\n{source}", generated.seed);
}

#[test]
fn generated_programs_keep_their_output_when_reordered_renamed_and_made_explicit() {
    let mut reordered = 0;
    for seed in 0..GENERATED_SEEDS {
        let generated = generate(seed, &GenConfig::default());
        assert_generated_variant(&generated, "original", &generated.source);

        let reversed = reverse_functions(&generated.source);
        if reversed != generated.source {
            reordered += 1;
        }
        assert_generated_variant(&generated, "functions reversed", &reversed);

        assert_generated_variant(&generated, "identifiers renamed", &rename_identifiers(&generated.source));

        let mut explicit = generated.program.clone();
        for function in &mut explicit.functions {
            if function.kind == FnKind::Pure {
                function.annotate = true;
            }
        }
        assert_generated_variant(&generated, "pure returns explicit", &explicit.print());
    }
    assert!(reordered > GENERATED_SEEDS / 2, "the reordering must change most programs");
}

#[test]
fn a_transformation_that_changes_meaning_is_detected() {
    let generated = generate(3, &GenConfig::default());
    let broken = generated.source.replacen("print ", "print \"changed \" + ", 1);
    let report = check_text("metamorphic.xsh", &broken);
    let changed = if report.accepted() {
        sandbox().run(&broken).expect("run the control").stdout != generated.expected
    } else {
        true
    };
    assert!(changed, "the control variant matched the original");
}

#[derive(Clone, Copy)]
struct Style {
    renamed: bool,
    reversed: bool,
    named: bool,
    extracted: bool,
}

const STYLES: [Style; 6] = [
    Style { renamed: false, reversed: false, named: false, extracted: false },
    Style { renamed: true, reversed: false, named: false, extracted: false },
    Style { renamed: false, reversed: true, named: false, extracted: false },
    Style { renamed: false, reversed: false, named: true, extracted: false },
    Style { renamed: false, reversed: false, named: false, extracted: true },
    Style { renamed: true, reversed: true, named: true, extracted: true },
];

struct Family {
    name: &'static str,
    expected: &'static str,
    build: fn(Style) -> Vec<String>,
}

fn names(style: Style, plain: &'static str, renamed: &'static str) -> &'static str {
    if style.renamed { renamed } else { plain }
}

fn relay(style: Style) -> Vec<String> {
    let inner = names(style, "inner", "carried");
    let outer = names(style, "outer", "handed");
    let value = names(style, "value", "subject");
    let call = if style.named { format!("{value}: {value}") } else { value.to_owned() };
    let body = if style.extracted {
        format!("let kept = {inner}({call})\n  kept")
    } else {
        format!("{inner}({call})")
    };
    let first = if style.named { format!("{value}: 7") } else { "7".to_owned() };
    let second = if style.named { format!("{value}: \"seven\"") } else { "\"seven\"".to_owned() };
    vec![
        format!("pure {inner}({value}) {{ {value} }}\n"),
        format!("pure {outer}({value}) {{\n  {body}\n}}\n"),
        format!("let number: Int = {outer}({first})\nlet word: Str = {outer}({second})\nprint $number\nprint $word\n"),
    ]
}

fn sizes(style: Style) -> Vec<String> {
    let size = names(style, "size", "extent");
    let item = names(style, "item", "thing");
    let arg = |text: &str| if style.named { format!("{item}: {text}") } else { text.to_owned() };
    let body = if style.extracted {
        format!("let measured = {item}.len()\n  measured")
    } else {
        format!("{item}.len()")
    };
    vec![
        format!("pure {size}({item}) {{\n  {body}\n}}\n"),
        format!(
            "let a = {size}({})\nlet b = {size}({})\nlet c = {size}({})\nprint $a\nprint $b\nprint $c\n",
            arg("[1, 2, 3]"),
            arg("b\"four\""),
            arg("{[\"k\"]: 1}")
        ),
    ]
}

fn applications(style: Style) -> Vec<String> {
    let apply = names(style, "apply", "invoke");
    let function = names(style, "function", "action");
    let value = names(style, "value", "argument");
    let double = names(style, "double", "twice");
    let call = |callee: &str, argument: &str| {
        if style.named { format!("{apply}({function}: {callee}, {value}: {argument})") } else { format!("{apply}({callee}, {argument})") }
    };
    let body = if style.extracted {
        format!("let result = {function}({value})\n  result")
    } else {
        format!("{function}({value})")
    };
    vec![
        format!("pure {apply}({function}, {value}) {{\n  {body}\n}}\n"),
        format!("pure {double}(n: Int) -> Int {{ n * 2 }}\n"),
        format!("let result = {}\nprint $result\n", call(double, "21")),
    ]
}

fn rows(style: Style) -> Vec<String> {
    let reader = names(style, "label_of", "caption_of");
    let row = names(style, "row", "record");
    let arg = |text: &str| if style.named { format!("{row}: {text}") } else { text.to_owned() };
    let body = if style.extracted {
        format!("let found = {row}.label\n  found")
    } else {
        format!("{row}.label")
    };
    vec![
        format!("pure {reader}({row}) {{\n  {body}\n}}\n"),
        format!(
            "let a = {}({})\nlet b = {}({})\nprint $a\nprint $b\n",
            reader,
            arg("{label: \"wide\", extra: 1}"),
            reader,
            arg("{label: \"narrow\"}")
        ),
    ]
}

fn fallbacks(style: Style) -> Vec<String> {
    let choose = names(style, "choose", "settle");
    let first = names(style, "first", "primary");
    let second = names(style, "second", "backup");
    let call = |a: &str, b: &str| {
        if style.named { format!("{choose}({first}: {a}, {second}: {b})") } else { format!("{choose}({a}, {b})") }
    };
    let body = if style.extracted {
        format!("let picked = {first} ?? {second}\n  picked")
    } else {
        format!("{first} ?? {second}")
    };
    vec![
        format!("pure {choose}({first}, {second}) {{\n  {body}\n}}\n"),
        format!(
            "let missing: Int? = null\nlet present: Int? = 4\nlet a = {}\nlet b = {}\nprint $a\nprint $b\n",
            call("missing", "9"),
            call("present", "9")
        ),
    ]
}

fn relayed_effects(style: Style) -> Vec<String> {
    let emit = names(style, "emit", "announce");
    let value = names(style, "value", "datum");
    let relay = names(style, "relay", "forward");
    let call = |text: &str| if style.named { format!("{value}: {text}") } else { text.to_owned() };
    let body = if style.extracted {
        format!("let returned = {emit}({value})\n  returned")
    } else {
        format!("{emit}({value})")
    };
    vec![
        format!("proc {emit}({value}) [io] {{\n  print ${value}\n  {value}\n}}\n"),
        format!("proc {relay}({value}) [io] {{\n  {body}\n}}\n"),
        format!(
            "let a = {relay}({})?\nlet b = {relay}({})?\nprint $a\nprint $b\n",
            call("3"),
            call("\"three\"")
        ),
    ]
}

fn defaults(style: Style) -> Vec<String> {
    let shift = names(style, "shift", "advance");
    let base = names(style, "base", "origin");
    let step = names(style, "step", "stride");
    let first = if style.named { format!("{base}: 5") } else { "5".to_owned() };
    let second = if style.extracted { format!("{first}, {step}: 1") } else { first.clone() };
    let third = if style.named { format!("{base}: 5, {step}: 3") } else { "5, 3".to_owned() };
    vec![
        format!("pure {shift}({base}, {step} = 1) {{\n  {base} + {step}\n}}\n"),
        format!("let a = {shift}({second})\nlet b = {shift}({third})\nprint $a\nprint $b\n"),
    ]
}

fn aliases(style: Style) -> Vec<String> {
    let alias = names(style, "Label", "Caption");
    let reader = names(style, "read_label", "read_caption");
    let row = names(style, "row", "entry");
    let transparent = names(style, "Again", "Repeat");
    let annotation = if style.extracted { transparent } else { alias };
    let call = if style.named { format!("{reader}({row}: {{text: \"alias\"}})") } else { format!("{reader}({{text: \"alias\"}})") };
    vec![
        format!("type {alias} = {{text: Str}}\ntype {transparent} = {alias}\n"),
        format!("pure {reader}({row}: {annotation}) -> Str {{\n  {row}.text\n}}\n"),
        format!("let a = {call}\nprint $a\n"),
    ]
}

const FAMILIES: [Family; 8] = [
    Family { name: "relay", expected: "7\nseven\n", build: relay },
    Family { name: "sizes", expected: "3\n4\n1\n", build: sizes },
    Family { name: "applications", expected: "42\n", build: applications },
    Family { name: "rows", expected: "wide\nnarrow\n", build: rows },
    Family { name: "fallbacks", expected: "9\n4\n", build: fallbacks },
    Family { name: "effects", expected: "3\nthree\n3\nthree\n", build: relayed_effects },
    Family { name: "defaults", expected: "6\n8\n", build: defaults },
    Family { name: "aliases", expected: "alias\n", build: aliases },
];

#[test]
fn generic_programs_keep_their_output_under_equivalent_rewrites() {
    for family in &FAMILIES {
        for style in STYLES {
            let mut declarations = (family.build)(style);
            if style.reversed {
                let uses = declarations.pop().expect("a family ends with its uses");
                declarations.reverse();
                declarations.push(uses);
            }
            let source = declarations.concat();
            let output = run(family.name, &source);
            assert_eq!(output, family.expected, "{} ({:?}):\n{source}", family.name, (style.renamed, style.reversed, style.named, style.extracted));
        }
    }
}

#[test]
fn a_generic_rewrite_that_changes_meaning_is_detected() {
    let family = &FAMILIES[4];
    let declarations = (family.build)(STYLES[0]);
    let source = declarations.concat();
    assert_eq!(run(family.name, &source), family.expected);
    let swapped = source.replace("choose(missing, 9)", "choose(9, missing)");
    assert_ne!(swapped, source, "the control rewrite must change the program");
    assert!(!check_text("metamorphic.xsh", &swapped).accepted(), "the swapped fallback must change acceptance");
}
