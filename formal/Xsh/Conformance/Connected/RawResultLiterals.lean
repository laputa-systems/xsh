import Xsh.Conformance.Connected.RawResult

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def resultSourceNames : List String :=
  ["Int", "Bool", "Str", "Result", "pure", "keep", "a", "return", "Ok", "refuse", "Err", "through", "r", "advance"]

def resultRows : RawInstructionRows :=
  ⟨[⟨"ExprParam", 31⟩, ⟨"ExprOk", 104⟩, ⟨"StmtReturn", 150⟩, ⟨"ExprStr", 24⟩, ⟨"ExprErr", 105⟩, ⟨"StmtReturn", 150⟩, ⟨"ExprParam", 31⟩, ⟨"ExprTry", 107⟩, ⟨"ExprOk", 104⟩, ⟨"StmtReturn", 150⟩, ⟨"ExprParam", 31⟩, ⟨"ExprTry", 107⟩, ⟨"ExprInt", 20⟩, ⟨"ExprBinary", 33⟩, ⟨"ExprOk", 104⟩, ⟨"StmtReturn", 150⟩],
    [false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false],
    [false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false],
    [⟨0, 1⟩, ⟨1, 1⟩, ⟨2, 1⟩, ⟨5, 1⟩, ⟨6, 2⟩, ⟨8, 1⟩, ⟨11, 1⟩, ⟨12, 2⟩, ⟨14, 1⟩, ⟨15, 1⟩, ⟨18, 1⟩, ⟨19, 2⟩, ⟨21, 2⟩, ⟨23, 4⟩, ⟨27, 1⟩, ⟨28, 1⟩],
    [0, 0, 1, 1, 2, 5, 3, 0, 4, 1, 5, 0, 6, 2, 7, 8, 1, 9, 0, 10, 3, 1, 0, 0, 11, 12, 4, 13, 14, 1, 15],
    [], [.add],
    [⟨0, 0⟩, ⟨182, 2⟩, ⟨256, 2⟩, ⟨256, 6⟩, ⟨0, 56⟩, ⟨57, 56⟩, ⟨114, 73⟩, ⟨188, 77⟩], [0, 0, 0, 0, 0, 0, 0, 0],
    [⟨0, 4⟩, ⟨4, 1⟩, ⟨5, 4⟩, ⟨9, 6⟩, ⟨15, 2⟩, ⟨17, 7⟩, ⟨24, 16⟩, ⟨40, 1⟩, ⟨41, 7⟩], [107, 101, 101, 112, 97, 97, 114, 103, 115, 114, 101, 102, 117, 115, 101, 110, 111, 116, 104, 114, 111, 117, 103, 104, 82, 101, 115, 117, 108, 116, 91, 73, 110, 116, 44, 32, 83, 116, 114, 93, 114, 97, 100, 118, 97, 110, 99, 101]⟩

def resultFunctions : RawFunctionRows :=
  ⟨[⟨⟨3, 2⟩, 4294967295, 1, 3, (0, 0, 0)⟩, ⟨⟨9, 2⟩, 4294967295, 2, 3, (0, 0, 0)⟩, ⟨⟨16, 2⟩, 4294967295, 3, 3, (0, 0, 0)⟩, ⟨⟨29, 2⟩, 4294967295, 4, 3, (0, 0, 0)⟩],
    [⟨1, .ground 1, ⟨0, 1⟩, ⟨0, 1⟩, 1, 2, ⟨0, 2⟩, 4⟩, ⟨4, .ground 2, ⟨1, 0⟩, ⟨1, 1⟩, 2, 1, ⟨2, 1⟩, 4⟩, ⟨6, .ground 3, ⟨1, 1⟩, ⟨2, 1⟩, 3, 2, ⟨3, 2⟩, 4⟩, ⟨9, .ground 3, ⟨2, 1⟩, ⟨3, 1⟩, 4, 2, ⟨5, 2⟩, 4⟩]⟩

def resultSignatures : List ResultSignature :=
  [⟨"keep", ⟨0, 38, 18⟩, [("a", .primitive .integer)], .result (.primitive .integer) (.primitive .string)⟩, ⟨"refuse", ⟨0, 91, 22⟩, [], .result (.primitive .integer) (.primitive .string)⟩, ⟨"through", ⟨0, 168, 19⟩, [("r", .result (.primitive .integer) (.primitive .string))], .result (.primitive .integer) (.primitive .string)⟩, ⟨"advance", ⟨0, 242, 23⟩, [("r", .result (.primitive .integer) (.primitive .string))], .result (.primitive .integer) (.primitive .string)⟩]

end Xsh.Conformance.Connected
