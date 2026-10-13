# XSH formal project

This is the Lean foundation for the XSH soundness campaign. It is pinned to
Lean 4.34.1 and uses only Lean's bundled libraries; no XSH runtime or product
build depends on it. The current foundation proves exact round trips for
32-bit words, binder IDs, and name IDs. It does not model or prove XSH
semantics.

From this directory, build the project and freshly recheck its root:

```sh
lake build
lake env leanchecker --fresh Xsh
```

Audit every declaration in the project modules:

```sh
scripts/audit-axioms.sh
scripts/audit-axioms.sh --self-test
```

The audit discovers every Lean source module under `formal/` outside
`scripts/`, builds and imports those modules, and checks declarations by their
module identity. This catches project declarations whose namespace is outside
`Xsh` and modules not imported by the root. It rejects unsafe declarations,
project axioms, and transitive dependencies beyond `propext`, `Quot.sound`,
and `Classical.choice`. The self-test checks that an axiom in an outside
namespace, `native_decide`, and `sorry` are rejected. Fresh kernel checking
and this allowlist audit are separate checks.

The bundled checker has no useful help output on this pinned build. Use the
documented `--fresh Xsh` invocation instead of probing its options.

The fixed source/IR corpus, scenario tests, and replay runner have not been
introduced yet. Direct Lake builds and the axiom controls above are the
available bootstrap checks; later campaign work will add those runners under
`formal/`.
