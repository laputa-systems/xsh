import Xsh.Conformance.Connected.ScopedOccurrences

namespace Xsh.Conformance.Connected

def actualScopedSource : String := "pure scoped(value) { var current = value; without time { current = value }; current }\nassert scoped(7) == 7\n"

def scopedTokens : List SourceToken := [
sourceToken 0 0 4 (.word "pure"),
 sourceToken 0 5 6 (.word "scoped"),
 sourceToken 0 11 1 (.symbol "("),
 sourceToken 0 12 5 (.word "value"),
 sourceToken 0 17 1 (.symbol ")"),
 sourceToken 0 19 1 (.symbol "{"),
 sourceToken 0 21 3 (.word "var"),
 sourceToken 0 25 7 (.word "current"),
 sourceToken 0 33 1 (.symbol "="),
 sourceToken 0 35 5 (.word "value"),
 sourceToken 0 40 1 (.symbol ";"),
 sourceToken 0 42 7 (.word "without"),
 sourceToken 0 50 4 (.word "time"),
 sourceToken 0 55 1 (.symbol "{"),
 sourceToken 0 57 7 (.word "current"),
 sourceToken 0 65 1 (.symbol "="),
 sourceToken 0 67 5 (.word "value"),
 sourceToken 0 73 1 (.symbol "}"),
 sourceToken 0 74 1 (.symbol ";"),
 sourceToken 0 76 7 (.word "current"),
 sourceToken 0 84 1 (.symbol "}"),
 sourceToken 0 85 1 (.newline),
 sourceToken 0 86 6 (.word "assert"),
 sourceToken 0 93 6 (.word "scoped"),
 sourceToken 0 99 1 (.symbol "("),
 sourceToken 0 100 1 (.integer "7"),
 sourceToken 0 101 1 (.symbol ")"),
 sourceToken 0 103 2 (.symbol "=="),
 sourceToken 0 106 1 (.integer "7"),
 sourceToken 0 107 1 (.newline)]

set_option maxRecDepth 100000 in
theorem scoped_source_tokenizes : tokenizeText 0 actualScopedSource = .ok scopedTokens := by
  rfl

theorem scoped_token_names : sourceNames scopedTokens =
    ["Int", "Bool", "Str", "Result", "pure", "scoped", "value", "var", "current", "without", "time", "assert"] := by
  decide +kernel

end Xsh.Conformance.Connected
