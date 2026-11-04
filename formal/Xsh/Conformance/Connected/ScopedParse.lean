import Xsh.Conformance.Connected.ScopedTokens

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def scopedName (id : Nat) (bound : id < 4294967296) : NameId := ⟨⟨id, bound⟩⟩
def scopedInput : NameId := scopedName 6 (by decide)
def scopedLocal : NameId := scopedName 8 (by decide)
def scopedFunctionName : NameId := scopedName 5 (by decide)
def scopedAssignment : Statement := .assign (.name scopedLocal) .set (.ident scopedInput)
def scopedRegion : Expr := .valueBlock [scopedAssignment]
def scopedBinding : Statement := .binding .var (.name scopedLocal) none (.ident scopedInput)
def scopedFunction : SourceFunction :=
  ⟨.pure, none, scopedFunctionName, ⟨0, 5, 6⟩, ⟨0, 19, 66⟩,
    [⟨scopedInput, ⟨0, 12, 5⟩, none⟩], none,
    [scopedBinding, .expression scopedRegion, .tailName scopedLocal],
    [(⟨0, 35, 5⟩, .ident scopedInput), (⟨0, 55, 19⟩, scopedRegion), (⟨0, 67, 5⟩, .ident scopedInput)],
    [(⟨0, 21, 20⟩, scopedBinding), (⟨0, 42, 32⟩, .expression scopedRegion),
     (⟨0, 57, 15⟩, scopedAssignment), (⟨0, 76, 7⟩, .tailName scopedLocal)],
    [⟨⟨0, 42, 12⟩, ⟨0, 55, 19⟩, [.time]⟩]⟩
def scopedCall : Expr := .call (.ident scopedFunctionName) [.positional (.literal (.integer "7"))]
def scopedAssert : Expr := .binary .eq scopedCall (.literal (.integer "7"))
def scopedUnit : SourceUnit := ⟨0,
  ["Int", "Bool", "Str", "Result", "pure", "scoped", "value", "var", "current", "without", "time", "assert"],
  [.function scopedFunction, .statement (.assert scopedAssert none)
    [(⟨0, 93, 14⟩, scopedAssert), (⟨0, 93, 9⟩, scopedCall),
     (⟨0, 93, 6⟩, .ident scopedFunctionName), (⟨0, 100, 1⟩, .literal (.integer "7")),
     (⟨0, 106, 1⟩, .literal (.integer "7"))]
    [(⟨0, 86, 22⟩, .assert scopedAssert none)]]⟩

set_option maxRecDepth 100000 in
theorem scoped_tokens_parse :
    parseDeclarations (32 * (scopedTokens.length + 1)) (sourceNames scopedTokens) scopedTokens =
      .ok scopedUnit.declarations := by
  rw [scoped_token_names]
  rfl

theorem actual_scoped_source_is_admitted : admitSource 0 actualScopedSource = .ok scopedUnit := by
  unfold admitSource
  rw [scoped_source_tokenizes]
  change (do
    let declarations ← parseDeclarations (32 * (scopedTokens.length + 1)) (sourceNames scopedTokens) scopedTokens
    pure (⟨0, sourceNames scopedTokens, declarations⟩ : SourceUnit)) = _
  rw [scoped_tokens_parse, scoped_token_names]
  rfl

end Xsh.Conformance.Connected
