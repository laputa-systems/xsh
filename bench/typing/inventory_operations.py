#!/usr/bin/env python3
"""Freeze the finite registry and language operation contracts from a settled tree."""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
DESTINATION = ROOT / "bench/typing/operations.json"
REGISTRY = "crates/xsh-registry/src"
ORIGINAL_REVISION = "877a114d6dc403db32c50f035b38aab7f193147a"
ORIGINAL_INVENTORY = ROOT / "bench/typing/history/877a114d/operations.json"
ORIGINAL_INVENTORY_SHA256 = "f3704071c065ba0ce0fa42320c2a75f936084f0527ecb0f81e449eb0d445e5bb"


def select_source_revision(build, requested=None):
    revision = build["build"]["source_revision"]
    if not isinstance(revision, str) or re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        raise ValueError("baseline build must identify a full exact source revision")
    if requested is not None and requested != revision:
        raise ValueError("requested source revision differs from the baseline build authority")
    return revision


def operation_contracts(operations):
    contracts = json.loads(json.dumps(operations))
    for operation in contracts:
        for owner in operation["authority"]:
            owner.pop("line", None)
    return contracts


def original_inventory():
    raw = ORIGINAL_INVENTORY.read_bytes()
    if hashlib.sha256(raw).hexdigest() != ORIGINAL_INVENTORY_SHA256:
        raise ValueError("original frozen inventory archive changed")
    inventory = json.loads(raw)
    if inventory["settled_source_revision"] != ORIGINAL_REVISION:
        raise ValueError("original inventory revision does not match its source authority")
    return inventory


def function_source(path, name):
    source = (ROOT / path).read_text()
    match = re.search(r"(?:pub\s+)?fn " + re.escape(name) + r"\(", source)
    if match is None:
        raise ValueError(f"missing authority {path}::{name}")
    opening = source.index("{", match.end())
    depth = 1
    cursor = opening + 1
    while depth:
        depth += (source[cursor] == "{") - (source[cursor] == "}")
        cursor += 1
    return source[match.start():cursor]


RUST_DUMPER = r'''
extern crate xsh_registry;
use xsh_registry::{RuntimeOp, signature::{self, MethodReceiver, ModuleFnSig}, types::Type};
#[derive(Debug)] enum Effect { Fs, Net, Process, Env, Time, Error, Io }
impl Effect { EFFECT_MODULE }
EFFECT_METHOD
fn q(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() { match c {
        '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"), '\t' => out.push_str("\\t"),
        c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)), c => out.push(c),
    }} out.push('"'); out
}
fn ty(value: &Type) -> String {
    match value {
        Type::BuiltinParameter(p) => format!("{{\"variable\":{}}}", q(p.label())),
        Type::List(a) | Type::Stream(a) | Type::Optional(a) => {
            let head = match value { Type::List(_) => "List", Type::Stream(_) => "Stream", _ => "Optional" };
            format!("{{\"head\":{},\"arguments\":[{}]}}", q(head), ty(a))
        },
        Type::Map(a,b) | Type::Result(a,b) => format!("{{\"head\":{},\"arguments\":[{},{}]}}",
            q(if matches!(value, Type::Map(_, _)) { "Map" } else { "Result" }), ty(a), ty(b)),
        Type::Record(fields) | Type::Module(fields) => format!("{{\"head\":{},\"fields\":{{{}}}}}",
            q(if matches!(value, Type::Record(_)) { "Record" } else { "Module" }),
            fields.iter().map(|(name,t)|format!("{}:{}",q(name),ty(t))).collect::<Vec<_>>().join(",")),
        Type::ErrorFamily(name) => format!("{{\"head\":\"ErrorFamily\",\"identity\":{}}}", q(name)),
        _ => format!("{{\"head\":{}}}",q(&format!("{:?}", value))),
    }
}
fn dump(kind: &str, domain: &str, name: &str, sig: &ModuleFnSig, receiver: Option<&Type>, effect: Option<Effect>, docs: &xsh_registry::api_docs::ApiDocs) {
    let params = sig.params.iter().map(|p|format!("{{\"label\":{},\"type\":{},\"defaulted\":{},\"mode\":\"positional_or_named\",\"rest\":false}}",q(p.name),ty(&p.ty),p.defaulted)).collect::<Vec<_>>().join(",");
    println!("{{\"kind\":{},\"domain\":{},\"name\":{},\"receiver_template\":{},\"parameters\":[{}],\"result\":{},\"pure\":{},\"command\":{},\"arg_check\":{},\"semantic_rule\":{},\"runtime_op\":{},\"binding\":{},\"required_effect\":{},\"contract\":{}}}",
        q(kind),q(domain),q(name),receiver.map(ty).unwrap_or("null".into()),params,ty(&sig.return_ty),sig.pure,sig.command,
        q(&format!("{:?}",sig.arg_check)),q(&format!("{:?}",sig.semantic_rule)),q(&format!("{:?}",sig.op)),q(&format!("{:?}",sig.binding)),
        effect.map(|e|q(&format!("{:?}",e).to_lowercase())).unwrap_or("null".into()),q(&docs.contract));
}
fn main() {
    let spec = signature::api_spec();
    for module in &spec.modules { for function in &module.sig.functions { for sig in &function.overloads {
        let id=signature::module_api_id(module.name,function.name);
        dump("module",module.name,function.name,sig,None,Effect::from_module_call(module.name,function.name),spec.docs(&id).unwrap());
    }}}
    for receiver in &spec.methods { for method in &receiver.methods { for overload in &method.overloads {
        let id=signature::method_api_id(receiver.receiver,method.name);
        dump("method",signature::receiver_name(receiver.receiver),method.name,&overload.sig,overload.receiver_ty.as_ref(),method_required_effect(receiver.receiver,overload.sig.op),spec.docs(&id).unwrap());
    }}}
    for (name,t) in xsh_registry::records::record_schemas() { println!("{{\"kind\":\"builtin_schema\",\"name\":{},\"schema\":{}}}",q(name),ty(&t)); }
    for family in xsh_registry::errors::builtin_error_families() { for variant in family.variants {
        let fields=family.fields.iter().map(|f|format!("{{\"label\":{},\"type\":{},\"mode\":\"positional_or_named\",\"defaulted\":false,\"rest\":false}}",q(f.name),ty(&f.ty))).collect::<Vec<_>>().join(",");
        println!("{{\"kind\":\"builtin_error_variant\",\"family\":{},\"name\":{},\"parameters\":[{}],\"facets\":{:?}}}",q(family.name),q(variant.name),fields,variant.facets);
    }}
    for (variant,stage) in [STAGE_ENTRIES] {
        let params=xsh_registry::stream_parameters::stage_parameters(stage).iter().map(|p|format!("{{\"label\":{},\"type\":{},\"default\":{},\"validation\":{},\"mode\":{}}}",q(p.name),q(&format!("{:?}",p.ty)),q(&format!("{:?}",p.default)),q(&format!("{:?}",p.validation)),q(if p.positional { "positional_or_named" } else { "named_only" }))).collect::<Vec<_>>().join(",");
        println!("{{\"kind\":\"stage\",\"variant\":{},\"name\":{},\"parameters\":[{}],\"accepts_static_callable\":{}}}",q(variant),q(stage),params,xsh_registry::stream_parameters::stage_accepts_callable(stage));
    }
}
'''


def enum_variants(name):
    source = (ROOT / "src/syntax/node.rs").read_text()
    body = re.search(r"pub enum " + name + r" \{([^}]+)\}", source).group(1)
    return re.findall(r"^\s*(\w+),", body, re.M)


def registry_rows():
    node_source = (ROOT / "src/syntax/node.rs").read_text()
    stage_source = node_source[node_source.index("impl StreamStageKind"):]
    stages = re.findall(r'Self::(\w+) => "([^"]+)"', stage_source.split("pub const fn is_adapter")[0])
    assert set(v for v, _ in stages) == set(enum_variants("StreamStageKind"))
    dumper = RUST_DUMPER.replace("EFFECT_MODULE", function_source("src/syntax/node.rs", "from_module_call"))
    dumper = dumper.replace("EFFECT_METHOD", function_source("src/modules/signature.rs", "method_required_effect"))
    dumper = dumper.replace("STAGE_ENTRIES", ",".join(f'("{v}","{s}")' for v, s in stages))
    by_configuration = {}
    with tempfile.TemporaryDirectory(prefix="xsh-operation-inventory-") as directory:
        directory = Path(directory)
        helper = directory / "dump.rs"
        helper.write_text(dumper)
        for configuration, flags in [("core", []), ("default", ["--cfg", 'feature="native-tests"'])]:
            library = directory / "libxsh_registry.rlib"
            subprocess.run(["rustc", "--edition=2024", "--crate-name", "xsh_registry", "--crate-type", "rlib", *flags,
                str(ROOT / REGISTRY / "lib.rs"), "-o", str(library)], check=True, capture_output=True)
            binary = directory / "dump"
            subprocess.run(["rustc", "--edition=2024", str(helper), "--extern", f"xsh_registry={library}", "-o", str(binary)], check=True, capture_output=True)
            result = subprocess.run([str(binary)], check=True, capture_output=True, text=True)
            by_configuration[configuration] = [json.loads(line) for line in result.stdout.splitlines()]
    return by_configuration


def walk_types(value):
    if isinstance(value, dict):
        if "head" in value or "variable" in value:
            yield value
        for item in value.values():
            yield from walk_types(item)
    elif isinstance(value, list):
        for item in value:
            yield from walk_types(item)


def type_text(value):
    if "variable" in value:
        return value["variable"]
    head = value["head"]
    if "arguments" in value:
        return head + "[" + ",".join(type_text(t) for t in value["arguments"]) + "]"
    if "fields" in value:
        return head + "{" + ",".join(k + ":" + type_text(v) for k, v in value["fields"].items()) + "}"
    return value.get("identity", head)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def authority(path, symbol):
    source = (ROOT / path).read_text()
    token = symbol.rsplit("::", 1)[-1]
    match = re.search(r"(?:fn|enum|struct) " + re.escape(token) + r"\b", source)
    return {"path": path, "symbol": symbol, "line": source[:match.start()].count("\n") + 1 if match else None}


def coverage(*owners):
    paths = [path for path in dict.fromkeys(owners) if (ROOT / path).is_file()]
    if not paths:
        raise ValueError(f"no existing coverage owner for {owners}")
    return {"positive_owners": paths, "negative_owners": paths + (["tests/sema.rs"] if "tests/sema.rs" not in paths else []),
        "status": "owners identified; per-entry witness audit and execution are separate acceptance work"}


def entry(identity, family, classification, owner, heads, parameters, relationship, result, errors, effects, evidence, tests, **extras):
    return {"id": identity, "family": family, "classification": classification, "authority": owner,
        "operand_receiver_heads": heads, "parameters": parameters, "relationship_template": relationship,
        "result_behavior": result, "error_behavior": errors, "effect_behavior": effects,
        "retained_requirement": {"eligible": classification == "sealed_requirement", "connected_signature_variables_only": True,
            "discharge": "unique finite checked operation; no runtime search" if classification == "sealed_requirement" else "ordinary template substitution or fixed explicit boundary"},
        "evidence_needs": evidence, "coverage": tests, **extras}


def registry_entry(row, overload_count):
    kind, domain, name = row["kind"], row["domain"], row["name"]
    types = list(walk_types([row["receiver_template"], row["parameters"], row["result"]]))
    variables = sorted({t["variable"] for t in types if "variable" in t})
    dynamic = [t for t in types if t.get("head") in ("Any", "Pure", "Proc") or t.get("head") in ("Record", "Module") and not t.get("fields")]
    semantic = row["semantic_rule"]
    if semantic in ("CliDescriptor", "CliCommands", "SchemaValidation", "ConstantKeyProjection"):
        classification = "dynamic_boundary"
        reason = "checked preparation facts can refine the declared dynamic contract; desired use alone cannot invent these facts"
    elif row["arg_check"] == "JsonCompatible" and row["result"].get("arguments", [{}])[0].get("head") != "Any":
        classification = "sealed_requirement"
        reason = "JSON-compatible concrete input is an existing sealed structural predicate; explicit Any remains a dynamic argument path without acquiring schema trust"
    elif variables:
        classification = "parametric_template"
        reason = "registry BuiltinTypeParameter identities connect receiver, arguments and result without receiver search"
    elif dynamic:
        classification = "dynamic_boundary"
        reason = "registry explicitly declares erased input/output or callable domains; concrete validation/signatures require an independent boundary"
    elif kind == "method":
        classification = "sealed_requirement"
        reason = "the finite receiver method table supplies a unique operation and exact signature once the receiver/labels are anchored"
    elif overload_count > 1:
        classification = "sealed_requirement"
        reason = "a statically resolved module callable has a finite declared overload family; connected signature variables may retain its unique-discharge requirement"
    else:
        classification = "monomorphic"
        reason = "module identity and fixed concrete signature determine this overload; overload selection remains finite"
    shape = {k: row[k] for k in ("kind", "domain", "name", "receiver_template", "parameters", "result", "runtime_op")}
    identifier = f"registry.{kind}.{domain}.{name}.{row['runtime_op']}.{digest(shape)[:16]}"
    path = REGISTRY + "/signature/" + ("methods.rs" if kind == "method" else "modules.rs")
    symbol = "value_methods" if kind == "method" else domain + "_module"
    if not re.search(r"fn " + re.escape(symbol) + r"\b", (ROOT / path).read_text()):
        symbol = "build_api_spec"
    receiver = row["receiver_template"]
    relationship = (type_text(receiver) if receiver else domain) + "(" + ",".join(p["label"] + ":" + type_text(p["type"]) for p in row["parameters"]) + ") -> " + type_text(row["result"])
    heads = sorted({t["head"] for t in types if "head" in t} | ({domain} if kind == "method" else set()))
    error = row["result"].get("arguments", [None, None])[1] if row["result"].get("head") == "Result" else None
    tests = "tests/xsh/stdlib/" + domain.lower() + ".xsh"
    if kind == "method":
        tests = "tests/xsh/stdlib/" + {"List": "methods", "Map": "map", "Record": "record", "Stream": "streams", "Str": "text", "Int": "methods", "Float": "methods", "EnvPathList": "env", "Status": "process", "Digest": "hash", "ProcessHandle": "process", "FsRoot": "fs_root_methods", "NetJob": "net", "PathConstructor": "path"}.get(domain, domain.lower()) + ".xsh"
    operation = entry(identifier, "receiver_method" if kind == "method" else "module_callable", classification,
        [authority(path, symbol), authority("src/sema/builtin_templates.rs", "BuiltinInstantiation"), authority("src/modules/signature.rs", "convert_method_sig" if kind == "method" else "convert_module_fn_sig")],
        heads, row["parameters"], relationship, {"declared": row["result"], "semantic_preparation": semantic},
        {"result_error": error, "propagation": "Result is data until explicit propagation; runtime validation remains operation-specific"},
        {"required": [row["required_effect"]] if row["required_effect"] else [], "pure": row["pure"], "kind": "pure" if row["pure"] else "proc", "latent": "Stream results retain producer lifecycle; creation does not imply pull"},
        ["checked RuntimeOp identity", "supplied/default/named-spread binding plan", "source argument evaluation order"] + (["fresh substitutions for " + ",".join(variables)] if variables else []) + (["prepared " + semantic + " facts"] if semantic != "Standard" else []),
        coverage(tests, "tests/xsh/builtin-templates.xsh" if variables else "tests/sema.rs", "crates/xsh-registry/src/signature/mod.rs"),
        classification_reason=reason, runtime_operation=row["runtime_op"], implementation_binding=row["binding"], argument_check=row["arg_check"], command_eligible=row["command"], contract=row["contract"], quantified_parameters=variables,
        receiver_template=receiver, overload_family_id=f"registry.{kind}.{domain}.{name}", overload_count=overload_count)
    requirements = []
    if kind == "method" and classification == "parametric_template":
        requirements.append("finite receiver-head selection when receiver head remains quantified")
    if variables and "K" in variables and "Map" in heads:
        requirements.append("MapKey(K) over the frozen scalar domains")
    if requirements:
        operation["retained_requirement"] = {"eligible": True, "connected_signature_variables_only": True,
            "conditional_requirements": requirements, "discharge": "ordinary type template plus unique finite receiver/key-domain evidence; no runtime search"}
    if domain in ("net", "NetJob"):
        operation["feature_behavior"] = {"default": "network-enabled implementation", "core": "signature retained; network transport operations report the feature-disabled boundary",
            "authority": "src/modules/net.rs and src/runtime/eval/lowered_run.rs cfg(feature=net) branches"}
    return operation


def language_entries(stages):
    entries = []
    expression = "src/sema/check/expr.rs"
    statement = "src/sema/check/stmt.rs"
    stream = "src/sema/check/stream.rs"
    call = "src/sema/check/call.rs"
    ordinary_errors = "checked domain rejection; runtime faults retain the existing source-attributed boundary"
    ordinary_effects = {"required": [], "operands": "effects of reached operands only"}

    def add(identity, family, classification, path, symbol, heads, parameters, relationship, result,
            tests, errors=ordinary_errors, effects=ordinary_effects, evidence=None, **extras):
        entries.append(entry("language." + identity, family, classification, [authority(path, symbol)],
            heads, parameters, relationship, result, errors, effects,
            evidence or ["checked domain/relationship", "fixed operation identity", "once-only source evaluation order"],
            coverage(*tests), **extras))

    def p(label, ty, mode="positional", **extras):
        return {"label": label, "type": ty, "mode": mode, **extras}

    binary_spellings = {"ResultFallback": "??", "Or": "or", "And": "and", "Eq": "==", "Ne": "!=",
        "Lt": "<", "Le": "<=", "Gt": ">", "Ge": ">=", "In": "in", "NotIn": "not in",
        "Add": "+", "Sub": "-", "Mul": "*", "Div": "/", "Rem": "%"}
    assert set(binary_spellings) == set(enum_variants("BinaryOp"))
    assert set(enum_variants("UnaryOp")) == {"Not", "Neg"}
    for variant in ("Eq", "Ne"):
        add("binary." + variant, "equality", "sealed_requirement", expression, "check_binary_arena",
            ["compatible checked types", "Optional", "Null", "nominal identities"], [p("left", "T"), p("right", "U")],
            "EqCompatible(T,U) -> Bool; preserve matches_expected compatibility, nullable cases, invariant containers and nominal identity",
            "Bool", ["tests/xsh/basic.xsh", "tests/xsh/enum-declarations.xsh", "tests/sema.rs"], syntax_variant=variant, spelling=binary_spellings[variant])
    for variant in ("Lt", "Le", "Gt", "Ge"):
        add("binary." + variant, "ordering", "sealed_requirement", expression, "check_binary_arena",
            ["Int", "UInt", "Float", "Duration", "Str"], [p("left", "T"), p("right", "T")],
            "Ordered(T), compatible operands -> Bool; numeric distinctions remain explicit", "Bool",
            ["tests/xsh/comparison-chain.xsh", "tests/xsh/duration-arithmetic.xsh", "tests/sema.rs"], syntax_variant=variant, spelling=binary_spellings[variant])
    add("comparison_chain", "ordering", "sealed_requirement", expression, "check_expr_with_schema_arena",
        ["Int", "UInt", "Float", "Duration", "Str"], [p("operands", "compatible ordered domains", rest=True)],
        "Adjacent Ordered(T) constraints -> Bool; each middle operand retained and evaluated once; short circuit",
        "Bool", ["tests/xsh/comparison-chain.xsh", "tests/sema.rs"])
    membership_domains = {"List": ("T", "List[T]", "value equality"), "Map": ("K", "Map[K,V]", "canonical scalar key identity"),
        "Record": ("Str", "{fields|R}", "exact field-name presence"), "Str": ("Str", "Str", "substring"),
        "Bytes": ("Bytes", "Bytes", "byte subsequence"), "Path": ("Str|Path", "Path", "substring of displayed Path text; Path needle uses displayed text; no filesystem access"),
        "EnvPathList": ("Path-like checked expression", "EnvPathList", "environment path membership")}
    for variant in ("In", "NotIn"):
        for head, (operand, container, semantics) in membership_domains.items():
            add("binary." + variant + "." + head, "membership", "sealed_requirement", expression, "check_binary_arena",
                [head], [p("left", operand), p("right", container)], f"{operand} {binary_spellings[variant]} {container} -> Bool; {semantics}",
                "Bool", ["tests/xsh/assertion-merge.xsh", "tests/xsh/typed-map-keys.xsh", "tests/xsh/stdlib/methods.xsh", "tests/sema.rs"],
                syntax_variant=variant, spelling=binary_spellings[variant])
    for variant in ("Add", "Sub", "Mul", "Div", "Rem"):
        domains = [("integer", "Int|UInt", "Int|UInt", "Int")]
        if variant != "Rem":
            domains.append(("float", "Float", "Float", "Float"))
        if variant == "Add":
            domains += [("text", "Str", "Str", "Str"), ("list", "List[T]", "List[T]", "List[T]"), ("duration", "Duration", "Duration", "Duration")]
        elif variant == "Sub":
            domains.append(("duration", "Duration", "Duration", "Duration"))
        elif variant == "Mul":
            domains += [("duration_scale", "Duration", "Int", "Duration"), ("duration_scale_reverse", "Int", "Duration", "Duration")]
        elif variant == "Div":
            domains += [("duration_scale", "Duration", "Int", "Duration"), ("duration_ratio", "Duration", "Duration", "Int")]
        for domain, left, right, result in domains:
            add("binary." + variant + "." + domain, "arithmetic", "sealed_requirement", expression, "check_binary_arena",
                [left, right], [p("left", left), p("right", right)], f"{left} {binary_spellings[variant]} {right} -> {result}", result,
                ["tests/xsh/duration-arithmetic.xsh", "tests/xsh/collections.xsh", "tests/xsh/uint-mutation.xsh", "tests/sema.rs"],
                errors="checked operands; Duration overflow/underflow/nonnegative multiplier/positive divisor; integer divide/modulo zero and overflow retain runtime errors; UInt destinations validate nonnegative storage",
                syntax_variant=variant, spelling=binary_spellings[variant])
    for variant, heads, relation, result in [("Not", ["Bool", "Status"], "Bool|Status -> Bool; retain Status success semantics", "Bool"),
            ("Neg", ["Int", "UInt", "Float"], "Int|UInt -> Int; Float -> Float", "Int|Float")]:
        add("unary." + variant, "unary_operator", "sealed_requirement", expression, "check_unary_arena", heads,
            [p("operand", "checked domain")], relation, result, ["tests/xsh/basic.xsh", "tests/xsh/uint-mutation.xsh", "tests/sema.rs"], syntax_variant=variant)
    for variant in ("And", "Or"):
        add("binary." + variant, "boolean_operator", "monomorphic", expression, "check_binary_arena", ["Bool"],
            [p("left", "Bool"), p("right", "Bool", "lazy")], "Bool * Bool -> Bool; reached-continuation refinements only", "Bool",
            ["tests/xsh/boolean-guards.xsh", "tests/xsh/proof-provenance.xsh", "tests/sema.rs"], syntax_variant=variant, spelling=binary_spellings[variant])
    for head in ("Result", "Optional"):
        add("binary.ResultFallback." + head, "fallback", "parametric_template", expression, "check_binary_arena", [head],
            [p("left", "Result[T,E]" if head == "Result" else "Optional[T]"), p("right", "T or |failure:E| -> T" if head == "Result" else "T", "lazy")],
            head + " payload and fallback share T; fallback evaluates only on failure/absence; Result handler parameter retains E", "T",
            ["tests/xsh/fallback-blocks.xsh", "tests/xsh/absence-lookups.xsh", "tests/sema.rs"], syntax_variant="ResultFallback", spelling="??")
    assert set(enum_variants("AssignOp")) == {"Set", "Add", "Sub", "Mul", "Div", "Rem"}
    for variant in enum_variants("AssignOp"):
        if variant == "Set":
            relation = "mutable lifetime T receives a contextually compatible T; existing nested Record/List/Map path; no type-changing updates"
            heads = ["mutable checked type"]
            symbol = "check_stmt_arena"
        else:
            domains = ["Int|UInt * Int|UInt -> Int"]
            if variant != "Rem":
                domains.append("Float * Float -> Float")
            if variant == "Add":
                domains += ["List[T] * List[T] -> List[T]", "Duration * Duration -> Duration"]
            if variant == "Sub":
                domains.append("Duration * Duration -> Duration")
            if variant in ("Mul", "Div"):
                domains.append("Duration * Int -> Duration")
            if variant == "Div":
                domains.append("Path * (Str|Path) -> Path")
            relation = "; ".join(domains) + "; assignment returns Unit; destination lifetime type and UInt checks persist"
            heads = [domain.split(" * ")[0] for domain in domains]
            symbol = "check_compound_assignment_op"
        add("assignment." + variant, "compound_arithmetic" if variant != "Set" else "assignment", "sealed_requirement" if variant != "Set" else "parametric_template",
            statement, symbol, heads, [p("target", "one mutable lifetime type", "lvalue"), p("rhs", "fixed compatible domain")], relation, "Unit",
            ["tests/xsh/list-assignment.xsh", "tests/xsh/uint-mutation.xsh", "tests/xsh/duration-arithmetic.xsh", "tests/sema.rs"],
            evidence=["selector evaluation once in path order, then RHS, then current root/old value", "atomic ancestor rebuild", "UInt storage boundary"], syntax_variant=variant)
    for head, source, item in [("List", "List[T]", "T"), ("Stream", "Stream[T]", "T"),
            ("Map", "Map[K,V]", "{key:K,value:V}"), ("Str", "Str", "Str (one Unicode scalar)"), ("Bytes", "Bytes", "Int (0..255)")]:
        for wrapped in (False, True):
            input_ty = "Result[" + source + ",E]" if wrapped else source
            add("iteration." + head + (".Result" if wrapped else ""), "iteration", "parametric_template" if head in ("List", "Stream", "Map") or wrapped else "monomorphic",
                "src/sema/types.rs", "iteration_item_type", [head] + (["Result"] if wrapped else []), [p("source", input_ty)], input_ty + " -> item " + item,
                item + "; for and comprehension binding", ["tests/xsh/scalar-iteration.xsh", "tests/xsh/comprehensions.xsh", "tests/xsh/typed-map-keys.xsh", "tests/sema.rs"],
                errors="outer Result source propagates E with the existing error boundary" if wrapped else ordinary_errors,
                effects={"required": ["error"] if wrapped else [], "latent": "Stream pulls contribute source work and close on all exits; source snapshots remain retained"},
                evidence=["item type and source kind", "source snapshot/cursor", "Map canonical scalar-key order", "loop/comprehension lexical control and cleanup"])
    for head in ("List", "Map"):
        add("comprehension." + head, "comprehension", "parametric_template", expression,
            "check_list_comp_arena" if head == "List" else "check_map_comp_arena", [head],
            [p("clauses", "ordered iteration/filter relationships"), p("projection", "T" if head == "List" else "K then V")],
            "nested clauses scope earlier bindings; Bool filters skip later clauses/projection; " + ("T -> List[T]" if head == "List" else "MapKey(K), K * V -> Map[K,V]"), head + "[T]" if head == "List" else "Map[K,V]",
            ["tests/xsh/comprehensions.xsh", "tests/xsh/typed-map-keys.xsh", "tests/xsh/scalar-iteration.xsh", "tests/sema.rs"])
    for head in ("List", "Stream"):
        add("yield_delegation." + head, "delegation", "parametric_template", statement, "check_yield_delegation_arena", [head],
            [p("source", head + "[T]")], "producer item T receives " + head + "[T]; direct Result/Map/Str/Bytes delegation rejected", "producer item T",
            ["tests/xsh/yield-delegation.xsh", "tests/sema.rs"], effects={"required": [], "latent": "delegated Stream work remains pull-time; child closes before parent"},
            evidence=["producer/item relationship", "delegated source kind and suspended frame", "resource context escape check"])
    add("yield", "producer_item", "parametric_template", statement, "check_yield_arena", ["producer item"], [p("value", "T")],
        "producer item and yielded value share T; no yield in defer or outside producer", "Unit inside producer",
        ["tests/xsh/yield-delegation.xsh", "tests/xsh/producers.xsh", "tests/sema.rs"], effects={"required": [], "latent": "body/default/yield work occurs only at established creation/pull boundaries"})
    for key in ("Str", "Int", "UInt", "Bool", "Bytes", "Path", "Duration"):
        add("map_key." + key, "map_keys", "sealed_requirement", "src/sema/types.rs", "is_map_key", [key], [p("key", key)],
            "MapKey(" + key + "); invariant Map[K,V] retains exact scalar domain, no display conversion",
            "canonical ordered scalar-key storage", ["tests/xsh/typed-map-keys.xsh", "tests/xsh/stdlib/map.xsh", "tests/sema.rs"],
            evidence=["exact scalar key domain", "prepared/runtime MapKey representation", "UInt nonnegative boundary"],
            storage_authority=authority("src/map_key.rs", "MapKey"))
    stage_contracts = {
        "where": ("T -> Bool", "Stream[T]"), "map": ("T -> U (U is not Unit; Result remains data)", "Stream[U]"),
        "par-map": ("T -> U (U is not Unit; Result remains data); worker configuration validated", "Stream[U]"),
        "each": ("T -> Unit|Result[Unit,E]; terminal explicit execution boundary", "Unit"),
        "batch": ("T -> batches; argv-compatible T required for byte/argv bounds; some limit required", "Stream[List[T]]"),
        "sort": ("Sortable(T): Int,Str,Bool,Path or records recursively composed of these", "Stream[T]"),
        "sort-by": ("T -> K; Sortable(K): Int,Str,Bool,Path or records recursively composed of these", "Stream[T]"),
        "take": ("nonnegative count", "Stream[T]"), "drop": ("nonnegative count", "Stream[T]"),
        "first": ("no block/configuration; first retained item", "Result[T,Error]"), "last": ("no block/configuration; final retained item", "Result[T,Error]"),
        "unique-by": ("T -> K; Result keys remain data", "Stream[T]"), "enumerate": ("no block/configuration", "Stream[{index:Int,value:T}]"),
        "zip": ("other: List[U]|Stream[U]; shorter source terminates", "Stream[{left:T,right:U}]"),
        "range": ("start:Int,end:Int; no block", "Stream[Int]"), "repeat": ("count:Int nonnegative; no block", "Stream[T]"),
        "tee": ("T -> Unit|Result[Unit,E]; existing explicit callback consumption", "Stream[T]"),
        "sum": ("T = Int; no block/configuration", "Int"), "min": ("existing runtime ordered item domain; no block/configuration", "Result[T,Error]"),
        "max": ("existing runtime ordered item domain; no block/configuration", "Result[T,Error]"),
        "group-by": ("T -> K; Result keys remain data", "Stream[{key:K,items:List[T]}]"),
        "fold": ("init:A; (A,T) -> A invariant; Result accumulator remains data; no implicit callback Result lifting", "A"),
        "reduce": ("init:A; (A,T) -> A invariant; Result accumulator remains data; no implicit callback Result lifting", "A"),
        "flat-map": ("T -> List[U]|Stream[U] with existing supported outer Result boundary", "Stream[U]"),
        "any": ("T -> Bool directly; no implicit Result predicate lifting", "Bool"), "all": ("T -> Bool directly; no implicit Result predicate lifting", "Bool"),
        "shuffle": ("seed:Int optional random; no block", "Stream[T]"), "table.print": ("T is checked record; columns:List[Str] optional; no block", "Unit"),
        "text.lines": ("first adapter only; input:Str; no block", "Stream[Str]"), "bytes.chunks": ("first adapter only; input:Bytes,size:Int positive; no block", "Stream[Bytes]"),
        "json.lines": ("first adapter only; input:Str; explicit unvalidated decoded item boundary", "Stream[Any]"),
        "json.stream": ("first adapter only; input:Str; explicit unvalidated decoded item boundary", "Stream[Any]"),
        "count": ("without block counts items; with T -> Str|Int|UInt|Bool, existing display-key conversion", "Int or Map[Str,Int]"),
        "collect": ("no block/configuration; consume source lifecycle", "List[T]"),
        "reduce-by": ("T -> {key:K,value:V}; exactly one sum/min/max mode; no implicit Result callback lifting; existing display-key conversion", "Map[Str,V]"),
    }
    assert set(stage_contracts) == {stage["name"] for stage in stages}
    for stage in stages:
        name = stage["name"]
        relation, result = stage_contracts[name]
        classification = "dynamic_boundary" if name in ("json.lines", "json.stream") else "sealed_requirement" if name in ("sort", "sort-by", "min", "max", "count", "reduce-by", "batch", "table.print") else "parametric_template"
        add("stage." + name, "structured_stage", classification, stream, "check_adapter_stage_arena" if name in ("text.lines", "bytes.chunks", "json.lines", "json.stream") else "check_stream_stage_arena",
            ["Str"] if name in ("text.lines", "json.lines", "json.stream") else ["Bytes"] if name == "bytes.chunks" else ["List[T]", "Stream[T]", "supported outer Result"],
            stage["parameters"], relation, result,
            ["tests/xsh/stdlib/streams.xsh", "tests/xsh/stdlib/stage-result-contracts.xsh", "tests/xsh/stdlib/fold-callbacks.xsh", "tests/xsh/stage-functions.xsh", "tests/sema.rs"],
            effects={"required": [], "latent": "callback/source effects retained until pulls; configuration evaluates once when stage is reached; terminal stages consume lifecycle"},
            evidence=["stage identity", "item/callback/result types and latent effects", "configuration argument binding/default/validation plan", "materialization and cleanup policy"],
            syntax_variant=stage["variant"], accepts_static_callable=stage["accepts_static_callable"],
            parameter_authority=authority(REGISTRY + "/stream_parameters.rs", "stage_parameters"))

    for form, classification, relationship, result, symbol in [
            ("direct_named", "parametric_template", "instantiate the declaration-owned scheme once; exact kind/labels/default/rest/argument/result relationships", "declared result and existing proc Result envelope", "check_call_arena"),
            ("checked_alias", "parametric_template", "retain checked signature independently of optional original declaration identity", "retained declared result", "check_call_arena"),
            ("module_contract", "parametric_template", "validated module callable fields retain monomorphic kind/parameters/result/effect contract", "contract result and existing invocation envelope", "check_call_arena"),
            ("rest_and_splice", "parametric_template", "fixed and rest element relationships; positional List splices preserve source evaluation order and bind exact callable slots", "declared callable result", "check_spread_call_arena")]:
        add("call." + form, "callable_application", classification, call, symbol, ["checked Pure", "checked Proc", "checked producer"],
            [p("callee", "rank-1 instantiation"), p("arguments", "checked required/default/rest slots", rest=True)], relationship, result,
            ["tests/xsh/callable-aliases.xsh", "tests/xsh/default-parameters.xsh", "tests/xsh/named-argument-spreading.xsh", "tests/sema.rs"],
            effects={"required": "retained callee invocation effects and reached default effects", "latent": "selection/storage/return does not execute callee/default work"},
            evidence=["checked callable signature", "optional declaration identity is independent", "binding/default/rest/spread plan", "selected callable handle/default slots"])
    for head, result, symbol in [("Pure", "Any", "check_pure_call_method_arena"), ("Proc", "Result[Any,Error]", "check_proc_call_method_arena")]:
        add("call.erased_" + head, "callable_application", "dynamic_boundary", "src/sema/check/method.rs", symbol, [head],
            [p("arguments", "dynamic", rest=True)], head + ".call keeps erased signature and existing envelope; no inferred trust", result,
            ["tests/xsh/dynamic-boundaries.xsh", "tests/xsh/default-parameters.xsh", "tests/sema.rs"],
            effects={"required": "unknown callable effects; restricted consumers need an independently checked contract"})
    add("call.stage_descriptor", "callable_descriptor", "non_generalizable", "src/sema/stage_arguments.rs", "stage_callable_argument",
        ["statically named function/proc"], [p("block", "checked one-item signature", "positional_or_named")],
        "descriptor must retain a unique static named declaration/import identity; computed typed callable can be invoked in an explicit existing block",
        "selected stage callback result", ["tests/xsh/stage-functions.xsh", "tests/xsh/callable-aliases.xsh", "tests/sema.rs"],
        classification_reason="static declaration identity is a required descriptor contract independent of type precision; rank-1 signature inference cannot manufacture identity",
        evidence=["declaration/import owner identity", "unique one-item callable signature", "stage-specific callback Result policy"])
    add("call.named_spread", "argument_binding", "parametric_template", "src/sema/arguments.rs", "expand_named_arguments", ["checked Record"],
        [p("spread", "visible checked fields", "named_spread")], "visible fields map to exact labels; occupancy/duplicate/required/default/rest constraints; spread receiver evaluates once", "ordinary checked callable result",
        ["tests/xsh/named-argument-spreading.xsh", "tests/xsh/generic-constructors.xsh", "tests/sema.rs"],
        evidence=["expanded visible field identities/types", "source order separate from slot order", "one receiver snapshot", "binding plan"])
    add("default.callable", "default", "parametric_template", statement, "check_function_arena", ["Pure", "Proc", "Stream"],
        [p("default", "parameter T", "omitted_slot")], "checked default and parameter share T; defaults resolve the declaration environment before parameter bindings exist, including earlier parameters; each selected default executes only for an omitted slot",
        "T supplied into fixed callable slot", ["tests/xsh/default-parameters.xsh", "tests/sema.rs"],
        effects={"required": "default effects at established invocation/producer boundary", "latent": "taking a callable does not execute defaults"},
        evidence=["default parameter scope and owning callable identity", "prepared default expression/slot", "argument/default order and skipped-argument laziness"])
    add("constructor.record", "constructor", "parametric_template", call, "check_inferred_record_constructor_arena", ["declared parametric record schema"],
        [p("fields", "schema-substituted named fields", "named_only")], "fresh declaration-owned arguments constrained exactly by supplied fields and independently anchored expected applications; phantom/null/empty ambiguity needs annotation",
        "declared schema application with concrete checked fields", ["tests/xsh/generic-constructors.xsh", "tests/xsh/parametric-records.xsh", "tests/xsh/record-constructors.xsh", "tests/sema.rs"],
        evidence=["canonical declaring/application identity", "fresh type arguments", "checked field binding", "schema-owned defaults and nominal/wire mappings"])
    add("default.record", "default", "parametric_template", "src/sema/constants.rs", "apply_prepared_defaults", ["declared record schema"],
        [p("default", "universal declared field T", "omitted_field")], "bounded literal constants from declaring module; universal defaults must work for all arguments and cannot select inferred arguments; constructors alone apply defaults",
        "independent constructed field value", ["tests/xsh/generic-constructors.xsh", "tests/xsh/record-constructors.xsh", "tests/xsh/constants.xsh", "tests/sema.rs"],
        evidence=["prepared literal constant", "declaring module identity", "universal default validation", "constructor supplied/omitted field plan"])
    for name, parameters, relation, result in [
            ("Ok", [p("value", "T", defaulted=True)], "expected success/error context preserves nested Result data; missing payload produces Unit", "Result[T,E]"),
            ("Err", [p("error", "E", "positional_or_named"), p("cause", "Error", "named_only", defaulted=True)], "outer nominal error E retained; cause is diagnostic metadata; success T must be independently anchored where executable reification needs it", "Result[T,E]"),
            ("Path", [p("text", "Str")], "explicit Str to Path boundary; NUL rejected; no implicit filesystem normalization", "Path"),
            ("range", [p("start_or_end", "Int"), p("end", "Int", defaulted=True)], "one/two Int bounds produce lazy integer stream", "Stream[Int]")]:
        add("constructor." + name, "constructor", "parametric_template" if name in ("Ok", "Err") else "monomorphic", call, "check_constructor_call_arena", [name], parameters,
            relation, result, ["tests/xsh/implicit-result-return.xsh", "tests/xsh/typed-causes.xsh", "tests/xsh/stdlib/path.xsh", "tests/xsh/stdlib/streams.xsh", "tests/sema.rs"])
    for name, relationship, symbol in [("tag", "declared nominal family identity and substituted positional payload types; aliases retain identity; no structural family conflation", "check_constructor_call_arena"),
            ("error_variant", "declared nominal family/variant/facets and payload labels; values remain error data until wrapped/propagated", "check_error_variant_constructor_arena")]:
        add("constructor." + name, "constructor", "parametric_template", call, symbol, ["declared nominal family"], [p("payload", "declared payload relationship")],
            relationship, "nominal declared value", ["tests/xsh/enum-declarations.xsh", "tests/xsh/typed-causes.xsh", "tests/sema.rs"],
            evidence=["declaring module/family/variant identity", "payload signature/binding", "wire-enum mapping where present"])
    add("require.explicit", "validation", "dynamic_boundary", expression, "check_expr_arena_inner", ["Any", "erased domain", "checked value"],
        [p("value", "independently evaluated source"), p("schema", "reifiable explicit schema")], "value.require(S) -> Result[S,Error]; prepared recursive validation/conversion is atomic; desired row accesses do not supply a schema",
        "Result[S,Error]", ["tests/xsh/dynamic-boundaries.xsh", "tests/xsh/inferred-require.xsh", "tests/xsh/enum-declarations.xsh", "tests/sema.rs"],
        evidence=["reifiable schema/application and declaring identities", "prepared validation walk", "nominal/wire/UInt/resource boundaries"], errors="validation produces Error data; explicit ? adds error propagation")
    add("require.contextual", "validation", "dynamic_boundary", "src/sema/check/expected.rs", "infer_requirement_target", ["independently typed destination"],
        [p("value", "dynamic source"), p("target", "independently anchored expected schema", "context")], "omitted require target accepted only from an independent reifiable destination; underconstrained inferred desire is rejected locally",
        "Result[S,Error]", ["tests/xsh/inferred-require.xsh", "tests/xsh/dynamic-boundaries.xsh", "tests/sema.rs"],
        evidence=["independent expected schema/application provenance", "prepared runtime validation operation"])
    add("descriptor.cli", "constructor_descriptor", "dynamic_boundary", "src/sema/constants.rs", "cli_descriptor_plan", ["prepared descriptor constants"],
        [p("descriptor", "bounded validated constant structure")], "literal/imported constant/alias forms share prepared descriptors; parser/default facts produce exact values/sources contracts; absent checked facts retain declared dynamic signature",
        "descriptor-determined checked record/envelope", ["tests/xsh/stdlib/cli_constants.xsh", "tests/xsh/stdlib/cli_commands_constants.xsh", "tests/xsh/constants.xsh", "tests/sema.rs"],
        evidence=["constant declaration/import identity", "prepared descriptor/default/parser plan", "checked result fields and independent schema contexts"])
    add("projection.field", "row_projection", "parametric_template", expression, "check_field_arena", ["checked Record", "checked Module"],
        [p("receiver", "{label:T|R}"), p("label", "fixed visible label")], "required visible label connects receiver row to T; wider actual rows remain preserved; private/absent fields do not gain a checked type",
        "T", ["tests/xsh/field-labels.xsh", "tests/xsh/record_binding.xsh", "tests/sema.rs"])
    add("projection.constant_key", "constant_key", "parametric_template", "src/sema/projection.rs", "resolve_constant_key_projection",
        ["checked Record", "checked Module"], [p("receiver", "{label:T|R}"), p("key", "prepared Str constant")],
        "prepared key selecting a visible checked field preserves T and callable/module export metadata through index/get; operation's existing fallible envelope remains",
        "T or Result[T,Error] by selected operation", ["tests/xsh/constant-key-projections.xsh", "tests/sema.rs"],
        evidence=["prepared key value/provenance", "checked visible field projection", "retained receiver/key/fallible operation"])
    for head, key, result in [("List", "Int", "T"), ("Map", "K", "V")]:
        add("index." + head, "collection_index", "parametric_template", expression, "check_index_arena", [head], [p("receiver", head + "[T]" if head == "List" else "Map[K,V]"), p("index", key)],
            "index preserves element/value relationship; invariant receiver domain", result, ["tests/xsh/list-assignment.xsh", "tests/xsh/typed-map-keys.xsh", "tests/sema.rs"],
            errors="negative/out-of-range List index and missing Map key retain existing runtime index failure")
    for head in ("List", "Str", "Bytes"):
        add("slice." + head, "collection_slice", "parametric_template" if head == "List" else "monomorphic", expression, "check_slice_arena", [head],
            [p("receiver", "List[T]" if head == "List" else head), p("start", "Int", defaulted=True), p("end", "Int", defaulted=True)],
            "half-open clamped slice; negative indices relative to length; reverse range empty; Str bounds count Unicode scalars", "List[T]" if head == "List" else head,
            ["tests/xsh/slicing.xsh", "tests/sema.rs"])
    add("postfix.optional", "optional_operation", "parametric_template", expression, "checked_postfix_receiver", ["Optional", "Result"],
        [p("receiver", "Optional[T]|Result[T,E]"), p("operation", "checked field/method/index/slice", "lazy")],
        "null skips entire operation including arguments/bounds; outer Result propagates once; optional methods returning Result retain both layers",
        "lifted checked operation result", ["tests/xsh/optional-postfix.xsh", "tests/xsh/absence-lookups.xsh", "tests/xsh/stdlib/methods.xsh", "tests/sema.rs"],
        evidence=["checked receiver layer and guard", "selected underlying operation", "skipped argument/bound plan", "Result propagation channel"])
    add("record.update", "row_update", "parametric_template", expression, "check_record_update_arena", ["checked Record"],
        [p("receiver", "{fields|R}", "leading_snapshot"), p("replacements", "existing disjoint field paths", "named_only")],
        "preserve complete actual row and schema; replacements match existing field types; no new/type-changing fields or default reapplication; publish only complete rebuild",
        "same receiver row/schema", ["tests/xsh/record-update.xsh", "tests/sema.rs"], evidence=["checked row/schema and field paths", "one receiver snapshot", "replacement source order", "atomic update plan"])
    for head, relationship in [("List", "elements and List splices share T; ordinary List element remains nested; empty contextual constraints"),
            ("Map", "MapKey(K), keys share K and values share V; Map spreads invariant; later duplicate wins; key before value"),
            ("Record", "unique visible labels with exact field types; contextual schema excess/missing checks; row spreads preserve fields")]:
        add("literal." + head, "collection_construction", "parametric_template", expression,
            {"List": "check_list_arena", "Map": "check_map_literal_arena", "Record": "check_record_arena"}[head], [head],
            [p("members", "element/field relationships", rest=True)], relationship, {"List": "List[T]", "Map": "Map[K,V]", "Record": "checked actual row"}[head],
            ["tests/xsh/collections.xsh", "tests/xsh/typed-map-keys.xsh", "tests/xsh/record_binding.xsh", "tests/sema.rs"])
    for family, relation, result in [("operators", "Any remains explicit dynamic; runtime operator domain check does not establish inferred concrete domains", "Any or Bool by operation"),
            ("membership", "Any/erased Record membership retains canonical runtime semantics without asserting row presence", "Bool"),
            ("projection", "dynamic index/field/method result cannot establish a concrete row/nominal/resource/effect promise", "Any"),
            ("pipeline", "Any pipeline input retains an erased item; operation-specific runtime guards remain", "Stream[Any] or terminal result")]:
        add("dynamic." + family, "dynamic_operation", "dynamic_boundary", expression if family != "pipeline" else stream,
            "check_expr_arena_inner" if family != "pipeline" else "stream_type_from_input", ["Any", "erased Record"], [p("operands", "dynamic domains")], relation, result,
            ["tests/xsh/dynamic-boundaries.xsh", "tests/xsh/stdlib/streams.xsh", "tests/sema.rs"], effects={"required": "opaque callable effects remain unknown where invocation occurs"})
    for variant in enum_variants("CoreCommand"):
        name = variant.lower()
        relation, result, required = {
            "Print": ("rest displayable Str/Int/Bool/Path scalars; first --flush controls inherited output", "Unit", []),
            "Eprint": ("rest displayable Str/Int/Bool/Path scalars; first --flush controls inherited error output", "Unit", []),
            "Cd": ("checked Path argument; cwd mutation remains explicit", "Result[Unit,Error]", ["env"]),
            "Env": ("checked environment assignments and optional scoped block; positional command args rejected", "Result[Unit,Error]", ["env"]),
        }[variant]
        add("command." + name, "core_command", "sealed_requirement" if variant in ("Print", "Eprint") else "monomorphic",
            "src/sema/check/command.rs", "check_core_command_arena", ["checked scalar command arguments"], [p("arguments", "existing command-word protocol", rest=True)],
            relation, result, ["tests/xsh/basic.xsh", "tests/xsh/stdlib/env.xsh", "tests/sema.rs"], effects={"required": required}, syntax_variant=variant)
    run_contracts = {
        "Plain": ("Unit in statement position; Status in value position", "statement nonzero exit propagates ProcessError; value nonzero is Status data"),
        "Status": ("Status", "nonzero is Status data; setup failure propagates ProcessError"),
        "CaptureText": ("Result[Str,ProcessError]", "nonzero/setup/decode failure is Err data"),
        "CaptureBytes": ("Result[Bytes,ProcessError]", "nonzero/setup failure is Err data"),
        "CaptureTextRecord": ("Result[{status:Status,stdout:Str,stderr:Str},ProcessError]", "nonzero is Ok record with Status; setup/decode failure is Err data"),
        "CaptureBytesRecord": ("Result[{status:Status,stdout:Bytes,stderr:Bytes},ProcessError]", "nonzero is Ok record with Status; setup failure is Err data"),
        "StreamText": ("Result[Stream[Str],ProcessError]", "pull-time child completion/decode failures retain ProcessError boundary"),
        "StreamBytes": ("Result[Stream[Bytes],ProcessError]", "pull-time child completion failures retain ProcessError boundary"),
    }
    assert set(run_contracts) == set(enum_variants("RunKind"))
    for variant, (result, errors) in run_contracts.items():
        add("run." + variant, "process_run", "monomorphic", expression, "check_run_expr_arena", ["argv-compatible scalar", "Bytes stdin stream"],
            [p("plan", "typed argv/redirect/pipeline contract"), p("accept", "bounded Int exit codes", "optional_policy")],
            "fixed run observation/envelope and accepted-exit policy; process arguments preserve exact boundaries", result,
            ["tests/xsh/run.xsh", "tests/xsh/run-accept.xsh", "tests/sema.rs"], errors=errors,
            effects={"required": ["process"], "latent": "Stream form owns child cleanup through pulls/cancellation"}, syntax_variant=variant)
    for name, relation, result, symbol in [("spawn", "single plain/status run or checked Command", "Result[ProcessHandle,ProcessError]", "check_spawn_form_arena"),
            ("wait", "ProcessHandle -> Status; List[ProcessHandle] -> List[Status]", "Result[Status|List[Status],ProcessError]", "check_wait_form_arena")]:
        add(name, "process_lifecycle", "sealed_requirement", expression, symbol, ["Command", "ProcessHandle", "List[ProcessHandle]"], [p("target", "fixed process contract")],
            relation, result, ["tests/xsh/stdlib/process.xsh", "tests/xsh/run.xsh", "tests/sema.rs"], effects={"required": ["process"]},
            evidence=["fixed target/operation kind", "handle ownership and process cleanup", "Result/status envelope"])
    add("postfix.propagate", "result_boundary", "parametric_template", "src/sema/check/types.rs", "check_propagation", ["Result"], [p("value", "Result[T,E]")],
        "unwrap T or propagate nominal E to nearest established try/retry/proc target; explicit boundary preserves Result nesting", "T",
        ["tests/xsh/result-contexts.xsh", "tests/xsh/value-blocks.xsh", "tests/sema.rs"], effects={"required": "error unless locally captured; other reached effects remain"},
        evidence=["Result success/error relationship", "lexical propagation target", "cleanup and outward error channel"])
    add("pattern.test", "pattern_operator", "parametric_template", expression, "check_expr_arena_inner", ["checked types", "dynamic subject", "nominal families"],
        [p("subject", "T"), p("pattern", "nonbinding literal/type/nominal constructor/facet pattern")], "subject is pattern -> Bool without unwrapping Result/Optional; bounded immutable proof may narrow a reached continuation",
        "Bool", ["tests/xsh/pattern-tests.xsh", "tests/xsh/proof-provenance.xsh", "tests/sema.rs"], evidence=["resolved pattern identity", "bounded versioned subject/projection provenance", "nominal payload relationship"])
    add("assertion", "statement_boundary", "monomorphic", statement, "check_assertion_statement", ["Bool"], [p("condition", "Bool"), p("message", "Str", "failure_only", defaulted=True)],
        "Bool statement/assert consumes a condition once and returns Unit; false propagates AssertionError; explicit message runs only on false", "Unit",
        ["tests/xsh/assertions.xsh", "tests/xsh/assert.xsh", "tests/xsh/assertion-tail-context.xsh", "tests/sema.rs"],
        effects={"required": "error unless captured; reached message/condition effects remain"}, evidence=["fixed assertion/value/discard use", "once-only reached operand evidence", "lexical cleanup/propagation target"])
    return entries


def supplemental_registry_entry(row):
    if row["kind"] == "builtin_schema":
        name = row["name"]
        return entry("registry.schema." + name, "builtin_schema_validation", "dynamic_boundary",
            [authority(REGISTRY + "/records.rs", "record_schemas"), authority("src/sema/check/expr.rs", "check_expr_arena_inner")],
            [name], [{"label": "value", "type": "dynamic source", "mode": "receiver"}, {"label": "schema", "type": name, "mode": "explicit_or_independently_expected"}],
            "explicit validation of registry-owned " + name + " shape; host record exposure is not a structural inference promise", "Result[" + name + ",Error]",
            "prepared validation returns Error data; no host access implied by schema validation", {"required": []},
            ["registry schema identity and exact fields", "prepared validation walk", "nested nominal/UInt/resource checks"],
            coverage("tests/xsh/dynamic-boundaries.xsh", "tests/sema.rs", "crates/xsh-registry/src/signature/mod.rs"), schema=row["schema"])
    family, name = row["family"], row["name"]
    return entry("registry.error." + family + "." + name, "builtin_nominal_error_variant", "monomorphic",
        [authority(REGISTRY + "/errors.rs", "builtin_error_families"), authority("src/sema/check/call.rs", "check_error_variant_constructor_arena")],
        [family], row["parameters"], "nominal " + family + "." + name + " with exact payload/facets; construction preserves error data identity",
        family + "." + name, "data until explicit Result wrapping/propagation", {"required": []},
        ["declaring nominal error family/variant", "payload types and facets", "source-constructor permission remains checked"],
        coverage("tests/xsh/typed-causes.xsh", "tests/xsh/assertions.xsh", "tests/xsh/run.xsh", "tests/sema.rs"), facets=row["facets"])


def generate(requested_revision=None):
    build = json.loads((ROOT / "bench/typing/baseline-build.json").read_text())
    revision = select_source_revision(build, requested_revision)
    original = original_inventory()
    configurations = registry_rows()
    overload_counts = collections.Counter((row["kind"], row["domain"], row["name"]) for row in configurations["default"] if row["kind"] in ("module", "method"))
    rows_by_key = {}
    for configuration, rows in configurations.items():
        for row in rows:
            key = digest(row)
            if key not in rows_by_key:
                rows_by_key[key] = {"row": row, "configurations": []}
            rows_by_key[key]["configurations"].append(configuration)
    entries = []
    stages = []
    for record in rows_by_key.values():
        row = record["row"]
        if row["kind"] == "stage":
            stages.append(row)
            continue
        operation = registry_entry(row, overload_counts[(row["kind"], row["domain"], row["name"])]) if row["kind"] in ("module", "method") else supplemental_registry_entry(row)
        operation["enabled_configurations"] = record["configurations"]
        entries.append(operation)
    language = language_entries(stages)
    for operation in language:
        operation["enabled_configurations"] = ["core", "default"]
    entries += language
    ids = [operation["id"] for operation in entries]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate stable operation identity")
    for operation in entries:
        for owner in operation["authority"]:
            if owner["line"] is None:
                raise ValueError(f"unresolved authority for {operation['id']}: {owner}")
        if operation["classification"] == "non_generalizable" and not operation.get("classification_reason"):
            raise ValueError("non-generalizable boundary lacks independent justification")
        if any("label" not in parameter or "mode" not in parameter for parameter in operation["parameters"]):
            raise ValueError(f"incomplete parameter contract for {operation['id']}")
    owner_paths = {owner["path"] for operation in entries for owner in operation["authority"]}
    owner_paths.update(str(path.relative_to(ROOT)) for path in (ROOT / REGISTRY).rglob("*.rs"))
    owner_paths.update(["src/syntax/node.rs", "src/modules/signature.rs", "src/modules/net.rs", "src/runtime/eval/lowered_run.rs", "src/runtime/eval/lowered_ops.rs", "src/sema/types.rs", "src/sema/stage_arguments.rs", "src/map_key.rs"])
    fingerprints = {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in sorted(owner_paths)}
    for path in fingerprints:
        frozen = subprocess.run(["git", "show", revision + ":" + path], cwd=ROOT, check=True, capture_output=True).stdout
        if hashlib.sha256(frozen).hexdigest() != fingerprints[path]:
            raise ValueError(f"operation authority {path} differs from settled revision {revision}; inventory cannot silently refreeze")
    old_contracts = operation_contracts(original["operations"])
    current_contracts = operation_contracts(sorted(entries, key=lambda operation: operation["id"]))
    if current_contracts != old_contracts:
        old_ids = {operation["id"] for operation in old_contracts}
        new_ids = {operation["id"] for operation in current_contracts}
        raise ValueError(f"frozen operation contracts changed; added={sorted(new_ids - old_ids)}, removed={sorted(old_ids - new_ids)}; review before changing the inventory")
    registry_paths = sorted(path for path in fingerprints if path.startswith(REGISTRY + "/"))
    for path in registry_paths:
        if fingerprints[path] != original["source_sha256"][path]:
            raise ValueError(f"registry authority changed at {path}; review before changing the inventory")
    changed_authorities = {path: {"original_sha256": original["source_sha256"][path], "current_sha256": fingerprint}
        for path, fingerprint in fingerprints.items() if fingerprint != original["source_sha256"][path]}
    registry_counts = {configuration: dict(sorted(collections.Counter(row["kind"] for row in rows).items())) for configuration, rows in configurations.items()}
    return {
        "schema_version": 1,
        "settled_source_revision": revision,
        "freeze_provenance": {
            "original_source_revision": ORIGINAL_REVISION,
            "original_inventory": str(ORIGINAL_INVENTORY.relative_to(ROOT)),
            "original_inventory_sha256": ORIGINAL_INVENTORY_SHA256,
            "current_source_authority": "bench/typing/baseline-build.json::build.source_revision",
            "reconciliation": "Source hashes and authority line locations updated after corrected ergonomics handoff; every operation contract, stable ID, classification, relationship, effect, evidence need, coverage owner and denominator is unchanged.",
            "operation_contract_sha256": digest(old_contracts),
            "operation_contract_comparison": "equal excluding authority line locations only",
            "registry_authority_comparison": "all registry Rust source files byte-identical, including RuntimeOp and default/core signatures",
            "registry_authority_paths": registry_paths,
            "changed_authorities": changed_authorities,
        },
        "scope": "exhaustive public registry overloads, builtin schemas/errors, every syntax operator/assignment/stage/run/core-command variant, and finite language relationship families",
        "generation": {"command": "python3 bench/typing/inventory_operations.py", "check_command": "python3 bench/typing/inventory_operations.py --check",
            "dependencies": "Python standard library, rustc, git; temporary dependency-free registry builds only", "identity": "registry callable IDs contain RuntimeOp and canonical signature SHA256 prefix, never table position"},
        "configurations": {"default": {"product_features": ["native-tests", "net", "tools"], "registry_features": ["native-tests"]},
            "core": {"product_features": [], "registry_features": [], "cargo_equivalent": "--no-default-features"}},
        "feature_contract": "net selects network implementation/disabled-error boundaries without removing registry signatures; tools does not change this registry; native-tests gates test helpers/schemas. Both registry configurations are compiled and enumerated independently.",
        "classification_contract": {
            "monomorphic": "fixed concrete operation signature; ordinary finite overload selection still applies",
            "parametric_template": "ordinary connected type/row/callable relationships; named schemes may quantify subject to value restriction",
            "sealed_requirement": "existing closed operation domain/structural predicate may be retained only when connected to quantified signature variables; concrete discharge must be unique",
            "dynamic_boundary": "explicit erased/validation/preparation boundary retained; checked facts may refine only through independent evidence",
            "non_generalizable": "independently required static descriptor identity; type precision cannot manufacture declaration identity"},
        "exhaustiveness": {"registry_rows_by_configuration": registry_counts, "syntax_enums": {name: enum_variants(name) for name in ("UnaryOp", "BinaryOp", "AssignOp", "StreamStageKind", "RunKind", "CoreCommand")},
            "operation_count": len(entries), "classifications": dict(sorted(collections.Counter(operation["classification"] for operation in entries).items())),
            "families": dict(sorted(collections.Counter(operation["family"] for operation in entries).items())),
            "coverage_status": "coverage owners frozen, not claims that every positive/negative acceptance witness has been executed",
            "excluded": ["removed migration-only APIs are not callable registry entries", "internal BridgeTypeName is an embedded implementation bridge, not a public language operation", "Unknown/Invalid/poison recovery states do not certify executable operations", "unknown/user-defined receiver search, unrestricted conversion ranking and higher-rank instantiation are outside the fixed language contract"]},
        "source_sha256": fingerprints,
        "operations": sorted(entries, key=lambda operation: operation["id"]),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare regenerated inventory without rewriting")
    parser.add_argument("--source-revision", help="exact revision; must match the immutable baseline build authority")
    arguments = parser.parse_args()
    result = generate(arguments.source_revision)
    serialized = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
    if arguments.check:
        manifest_revision = json.loads((ROOT / "bench/typing/manifest.json").read_text())["baseline_commit"]
        if manifest_revision != result["settled_source_revision"]:
            raise SystemExit("manifest baseline revision differs from the inventory/build source authority")
        if DESTINATION.read_text() != serialized:
            raise SystemExit("operation inventory differs; frozen contract must be reviewed")
    else:
        DESTINATION.parent.mkdir(parents=True, exist_ok=True)
        DESTINATION.write_text(serialized)
    print(json.dumps({"checked" if arguments.check else "written": str(DESTINATION.relative_to(ROOT)), "operations": result["exhaustiveness"]["operation_count"], "registry": result["exhaustiveness"]["registry_rows_by_configuration"]}))


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.stderr.decode() if isinstance(error.stderr, bytes) else error.stderr) from error
