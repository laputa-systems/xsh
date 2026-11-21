import Xsh.Semantics.CoreWorld

namespace Xsh.Validation.Core

open Xsh.Core

variable {Declaration Identity Source : Type}

def eraseValidated : ValueType Declaration → Value Declaration Identity Source → Value Declaration Identity Source
  | .nominal _ _, .nominal _ contents => contents
  | _, candidate => candidate

end Xsh.Validation.Core
