//! A deterministic corpus of registry-signature probes.
//!
//! For every pure, effect-free standard method and module function, each
//! overload is called with sample arguments of its declared types in every
//! calling convention: positional, positional without defaults, named in
//! declaration and reverse order, a record spread, and positional-then-named.
//! A probe the checker rejects is fine; a probe it accepts must prepare and
//! run without an internal error or a runtime type error. Domain failures
//! (`"a".parse_int()`) are ordinary data.

use crate::harness::{Sandbox, check_text, internal_marker, runtime_type_error};
use xsh_registry::signature::{MethodReceiver, ModuleFnSig, api_spec};
use xsh_registry::types::{BuiltinTypeParameter, Type};

#[derive(Clone, Debug)]
pub struct Probe {
    /// `Receiver.method#overload/convention` or `module.function#...`.
    pub label: String,
    pub call: String,
}

/// Receiver element/key/value instantiation for one probe.
#[derive(Clone)]
struct Inst {
    element: Type,
    key: Type,
    value: Type,
}

fn sample(ty: &Type, inst: &Inst) -> Option<String> {
    Some(match ty {
        Type::Int => "2".into(),
        Type::Float => "1.5".into(),
        Type::Str => "\"a,b c\"".into(),
        Type::Bool => "true".into(),
        Type::Bytes => "b\"ab\\x0acd\"".into(),
        Type::Path => "p\"dir/name.txt\"".into(),
        Type::Duration => "1s".into(),
        Type::Null => "null".into(),
        Type::Regex => "rx\"a+\"".into(),
        Type::List(inner) => format!("[{}]", sample(inner, inst)?),
        Type::Map(key, value) => format!("{{[{}]: {}}}", sample(key, inst)?, sample(value, inst)?),
        Type::Optional(inner) => sample(inner, inst)?,
        Type::Record(fields) if !fields.is_empty() => {
            let mut parts = Vec::new();
            for (name, ty) in fields {
                parts.push(format!("{name}: {}", sample(ty, inst)?));
            }
            format!("{{{}}}", parts.join(", "))
        }
        Type::Result(ok, _) => format!("Ok({})", sample(ok, inst)?),
        Type::BuiltinParameter(BuiltinTypeParameter::Element) => {
            sample(&inst.element.clone(), inst)?
        }
        Type::BuiltinParameter(BuiltinTypeParameter::Key) => sample(&inst.key.clone(), inst)?,
        Type::BuiltinParameter(BuiltinTypeParameter::Value) => sample(&inst.value.clone(), inst)?,
        _ => return None,
    })
}

fn receiver_text(
    receiver: MethodReceiver,
    receiver_ty: Option<&Type>,
    inst: &Inst,
) -> Option<String> {
    if let Some(ty) = receiver_ty {
        return match ty {
            Type::List(_) | Type::Map(..) => sample(ty, inst),
            _ => None,
        };
    }
    Some(match receiver {
        MethodReceiver::Str => "\"a,b c\\nd\"".into(),
        MethodReceiver::Int => "6".into(),
        MethodReceiver::Float => "2.5".into(),
        MethodReceiver::Bytes => "b\"ab\\x0acd\"".into(),
        MethodReceiver::Path => "p\"dir/name.txt\"".into(),
        MethodReceiver::List => format!(
            "[{}, {}]",
            sample(&inst.element, inst)?,
            sample(&inst.element, inst)?
        ),
        MethodReceiver::Map => format!(
            "{{[{}]: {}}}",
            sample(&inst.key, inst)?,
            sample(&inst.value, inst)?
        ),
        MethodReceiver::Record => "{a: 1, b: \"x\"}".into(),
        MethodReceiver::Regex => "rx\"a+\"".into(),
        _ => return None,
    })
}

/// The argument lists for every calling convention of one overload.
fn conventions(sig: &ModuleFnSig, inst: &Inst) -> Vec<(&'static str, String)> {
    let mut values = Vec::new();
    for param in &sig.params {
        match sample(&param.ty, inst) {
            Some(value) => values.push((param.name, value, param.defaulted)),
            None => return Vec::new(),
        }
    }
    let positional = |items: &[(&str, String, bool)]| {
        items
            .iter()
            .map(|(_, value, _)| value.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let named = |items: &[(&str, String, bool)]| {
        items
            .iter()
            .map(|(name, value, _)| format!("{name}: {value}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let required: Vec<_> = values
        .iter()
        .filter(|(_, _, defaulted)| !defaulted)
        .cloned()
        .collect();
    let mut out = vec![("positional", positional(&values))];
    if required.len() != values.len() {
        out.push(("required", positional(&required)));
    }
    if !values.is_empty() {
        out.push(("named", named(&values)));
        let mut reversed = values.clone();
        reversed.reverse();
        out.push(("named-reversed", named(&reversed)));
        out.push(("spread", format!("...{{{}}}", named(&values))));
        if values.len() > 1 {
            out.push(("mixed", format!("{}, {}", values[0].1, named(&values[1..]))));
        }
    }
    out
}

fn instantiations(receiver: MethodReceiver) -> Vec<Inst> {
    match receiver {
        MethodReceiver::List => vec![
            Inst {
                element: Type::Int,
                key: Type::Str,
                value: Type::Int,
            },
            Inst {
                element: Type::Str,
                key: Type::Str,
                value: Type::Str,
            },
        ],
        MethodReceiver::Map => vec![
            Inst {
                element: Type::Int,
                key: Type::Str,
                value: Type::Int,
            },
            Inst {
                element: Type::Int,
                key: Type::Int,
                value: Type::List(Box::new(Type::Str)),
            },
        ],
        _ => vec![Inst {
            element: Type::Int,
            key: Type::Str,
            value: Type::Int,
        }],
    }
}

fn effect_free_method(receiver: MethodReceiver, name: &str, overload: usize) -> bool {
    xsh::api::api_spec()
        .method_overloads(receiver, name)
        .and_then(|overloads| overloads.get(overload))
        .is_some_and(|method| method.sig.pure && method.sig.effect.is_none() && !method.sig.command)
}

fn effect_free_function(module: &str, name: &str, overload: usize) -> bool {
    xsh::api::api_spec()
        .module_overloads(module, name)
        .and_then(|overloads| overloads.get(overload))
        .is_some_and(|function| function.pure && function.effect.is_none() && !function.command)
}

/// The full probe corpus, in registry order.
pub fn corpus() -> Vec<Probe> {
    let spec = api_spec();
    let mut probes = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    for receiver in &spec.methods {
        for method in &receiver.methods {
            for (index, overload) in method.overloads.iter().enumerate() {
                if !effect_free_method(receiver.receiver, method.name, index) {
                    continue;
                }
                for inst in instantiations(receiver.receiver) {
                    let Some(recv) =
                        receiver_text(receiver.receiver, overload.receiver_ty.as_ref(), &inst)
                    else {
                        continue;
                    };
                    for (convention, args) in conventions(&overload.sig, &inst) {
                        let call = format!("{recv}.{}({args})", method.name);
                        if seen.insert(call.clone()) {
                            probes.push(Probe {
                                label: format!(
                                    "{:?}.{}#{index}/{convention}",
                                    receiver.receiver, method.name
                                ),
                                call,
                            });
                        }
                    }
                }
            }
        }
    }
    let inst = Inst {
        element: Type::Int,
        key: Type::Str,
        value: Type::Int,
    };
    for module in &spec.modules {
        for function in &module.sig.functions {
            for (index, overload) in function.overloads.iter().enumerate() {
                if !effect_free_function(module.name, function.name, index) {
                    continue;
                }
                for (convention, args) in conventions(overload, &inst) {
                    let call = format!("{}.{}({args})", module.name, function.name);
                    if seen.insert(call.clone()) {
                        probes.push(Probe {
                            label: format!(
                                "{}.{}#{index}/{convention}",
                                module.name, function.name
                            ),
                            call,
                        });
                    }
                }
            }
        }
    }
    probes
}

/// One program running `probes` under an empty effect clause. Each probe's
/// outcome is printed on its own line; a captured failure prints its message
/// so internal errors and type disagreements stay visible.
pub fn program(probes: &[&Probe]) -> String {
    let mut text = String::from(
        "proc fuzz_main() [] -> Result[List[Str]] {\n  try {\n    var out: List[Str] = []\n",
    );
    for (index, probe) in probes.iter().enumerate() {
        text.push_str(&format!(
            "    let probe{index} = try {{\n      let _ = {}\n    }}\n    out += [match probe{index} {{ Ok(_) => \"{index} ok\", Err(failure) => \"{index} err \" + failure.message }}]\n",
            probe.call
        ));
    }
    text.push_str("    out\n  }\n}\nfor line in fuzz_main()? { print ${line} }\n");
    text
}

#[derive(Debug, Default)]
pub struct Summary {
    pub probes: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub failures: Vec<(Probe, String)>,
}

fn suspicious(text: &str) -> Option<String> {
    if let Some(marker) = internal_marker(text) {
        return Some(format!("internal ({marker})"));
    }
    if runtime_type_error(text) {
        return Some("runtime type error".into());
    }
    None
}

/// Checks every probe and runs the accepted ones, one program per batch;
/// a failing batch is rerun probe by probe to attribute the failure.
pub fn run_corpus(probes: &[Probe], sandbox: &Sandbox, batch: usize) -> Summary {
    let mut summary = Summary {
        probes: probes.len(),
        ..Summary::default()
    };
    let mut accepted = Vec::new();
    for probe in probes {
        let report = check_text("probe.xsh", &program(&[probe]));
        if let Some(internal) = report.internal_error() {
            summary.failures.push((probe.clone(), internal));
        } else if report.accepted() {
            accepted.push(probe);
        } else {
            summary.rejected += 1;
        }
    }
    summary.accepted = accepted.len();
    for group in accepted.chunks(batch.max(1)) {
        let failures = run_group(group, sandbox);
        if failures.is_empty() {
            continue;
        }
        if group.len() == 1 {
            summary.failures.extend(failures);
        } else {
            for probe in group {
                summary.failures.extend(run_group(&[probe], sandbox));
            }
        }
    }
    summary
}

fn run_group(group: &[&Probe], sandbox: &Sandbox) -> Vec<(Probe, String)> {
    let source = program(group);
    let run = match sandbox.run(&source) {
        Ok(run) => run,
        Err(error) => return vec![((*group[0]).clone(), format!("spawn failed: {error}"))],
    };
    let mut failures = Vec::new();
    if run.timed_out || run.signal.is_some() || run.memory_exceeded.is_some() {
        let reason = if let Some(footprint) = run.memory_exceeded {
            format!("exceeded the memory limit at {} MiB", footprint >> 20)
        } else if run.timed_out {
            "timed out".to_string()
        } else {
            format!("killed by signal {:?}", run.signal)
        };
        failures.push(((*group[0]).clone(), format!("{reason}\n{}", run.stderr)));
        return failures;
    }
    if run.status != Some(0) {
        // A runtime failure outside a Result (`b"".dump("bad")`) ends the
        // program; it is a defect only when it is internal or a type error.
        // A batch reruns probe by probe, so later probes still run.
        if let Some(reason) = suspicious(&run.stderr) {
            failures.push(((*group[0]).clone(), format!("{reason}\n{}", run.stderr)));
        } else if group.len() > 1 {
            for probe in group {
                failures.extend(run_group(&[probe], sandbox));
            }
        }
        return failures;
    }
    for line in run.stdout.lines() {
        let Some((index, rest)) = line.split_once(' ') else {
            continue;
        };
        if let (Ok(index), Some(reason)) = (index.parse::<usize>(), suspicious(rest))
            && let Some(probe) = group.get(index)
        {
            failures.push(((*probe).clone(), format!("{reason}: {rest}")));
        }
    }
    failures
}
