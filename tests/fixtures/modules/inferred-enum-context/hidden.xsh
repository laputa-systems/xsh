##! A module with a private enum.
enum Choice { Same, HiddenOther }
## Return a private choice.
export pure choice() -> Choice { Same }
