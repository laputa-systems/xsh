**No formal grammar**

SPEC.md mixes philosophy, normative semantics, and grammar in prose. That works now. In 30 years when someone implements XSH on a new platform or wants to build a syntax-aware tool, they need a machine-readable BNF or EBNF — not because prose is wrong, but because prose is ambiguous under adversarial reading and doesn't survive translation. SQL lasted 40 years in part because ISO standardized a grammar that implementors could argue about precisely. The formatter is an implicit grammar oracle right now, which is clever but fragile. A formal grammar doc would also make the "spec-first" commitment enforceable beyond this implementation.

---

**No stability tiers**

The spec says it's authoritative, and LANG.md has a promotion process. But there's no explicit stability contract: what's frozen forever, what's experimental, what can change with a deprecation window? For a 100-year language this matters enormously. Rust solves this with Editions; Go with its compatibility promise. Without it, "spec-first" just means "whoever writes the spec owns the language" — fine for now, fragile as the project outlives its authors. Even just three tiers (stable / provisional / experimental) with clear promotion rules would give future maintainers something to argue from.

---

**The capability/effect system is half-finished**

The effect system (`error`, and presumably `io` and others) is the seed of something genuinely important: being able to say what a script *can't* do. But from what I read it's used mainly for purity checking, not as a real capability model. For a shell that will run as PID1, execute untrusted package builds, and handle auth, you eventually need: "this proc may only read the filesystem, not spawn processes" — enforceable, not advisory. The effect system could be that, but it needs to be designed for security, not just type checking. Right now it feels like it's headed toward "lint for side effects" rather than "enforced isolation."

We'd likely just implement this on linux only and leverage cgroups v2.

---

**Module versioning is absent**

`config.ini` and module imports exist. But for a language meant to outlive its authors: how do you pin a module version? How do module authors signal breaking changes? How does a script written in 2026 run in 2046 when `fs.walk` has new fields in its record schema? The type system's width compatibility helps somewhat, but there's no versioning story. This is the hardest problem in language design for longevity — Python still hasn't fully solved it — but it needs at least a philosophy statement now, before the ecosystem forms.

---

**Determinism is unspecified**

For a tool building reproducible packages, determinism matters: Is map iteration order stable? Are there any sources of non-determinism in the runtime besides explicit process invocation? This should be normative. "XSH is deterministic given the same inputs and environment" (or isn't, and here's why) needs to be a spec statement, not an assumption.

---

**The upper bound is undocumented**

The philosophy says XSH fills the gap between bash and a full application runtime. But for a 100-year language, you also need the anti-use-cases: when should someone *not* use XSH? Where does it hand off to something else? This isn't just positioning — it's what tells future users when they've pushed the tool past its design envelope, and it prevents the language from accumulating features that blur the original vision. "This is not the right tool when..." is as important as "this is the right tool for..."
