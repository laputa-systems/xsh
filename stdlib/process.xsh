##! Embedded implementation of the public `process` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# argument-word parser lives here. Native process creation and inspection stay
# native.
#
# Every binding in this file has a unique name inside its function: rebinding a
# name that an enclosing scope already declared is miscompiled today (the inner
# binding is initialized once from the outer value and never updates), so the
# state machine spells out one name per value.

# The only rejection the parser reports. A declared error family derives its
# error kind from `family.variant`, so the variant carries `kind` as a payload
# field: the runtime reads a string `kind` field as the error kind, which keeps
# the public kind exactly `argv-words`.
error ArgvWordsError = Rejected(kind: Str, message: Str)

# Byte width of the whitespace character starting at `index`, or zero when that
# position does not start a whitespace character.
#
# The baseline classifies with `char::is_whitespace`, the Unicode `White_Space`
# property: the ASCII space, the tab/line-feed/vertical-tab/form-feed/
# carriage-return run, U+0085, U+00A0, U+1680, U+2000..U+200A, U+2028, U+2029,
# U+202F, U+205F, and U+3000. `Str` indexes bytes, so the one-, two-, and
# three-byte members of that set are matched by their UTF-8 bytes; every
# remaining character is either ASCII or not whitespace at all.
pure whitespace_width(text: Str, index: Int) -> Int {
  let first = text.byte_at(index, -1)
  if first == 32 or (first >= 9 and first <= 13) {
    return 1
  }
  if first == 194 {
    # U+0085, U+00A0
    let second = text.byte_at(index + 1, -1)
    if second == 133 or second == 160 {
      return 2
    }
    return 0
  }
  if first == 225 {
    # U+1680
    if text.byte_at(index + 1, -1) == 154 and text.byte_at(index + 2, -1) == 128 {
      return 3
    }
    return 0
  }
  if first == 226 {
    # U+2000..U+200A, U+2028, U+2029, U+202F, and U+205F
    let next = text.byte_at(index + 1, -1)
    if next == 128 {
      let last = text.byte_at(index + 2, -1)
      if (last >= 128 and last <= 138) or last == 168 or last == 169 or last == 175 {
        return 3
      }
    } else if next == 129 and text.byte_at(index + 2, -1) == 159 {
      return 3
    }
    return 0
  }
  if first == 227 {
    # U+3000
    if text.byte_at(index + 1, -1) == 128 and text.byte_at(index + 2, -1) == 128 {
      return 3
    }
    return 0
  }
  return 0
}

# Byte width of the UTF-8 character starting at `index`.
#
# Text is always valid UTF-8, so the leading byte decides. A byte that cannot
# start a character reports width one, keeping the scan moving forward.
pure character_width(text: Str, index: Int) -> Int {
  let first = text.byte_at(index, -1)
  if first < 194 {
    return 1
  }
  if first < 224 {
    return 2
  }
  if first < 240 {
    return 3
  }
  return 4
}

# The rejection message for the shell syntax character at `index`.
#
# Every character in the set is a single ASCII byte, so the message quotes the
# offending byte out of the input instead of a byte-to-character table.
pure syntax_message(text: Str, index: Int) -> Str {
  return "shell syntax character `" + text.byte_slice(index, 1) + "` is not accepted"
}

# A rejected `argv_words` result, carrying the public rejection kind.
pure rejected(message: Str) -> Result[List[Str]] {
  return Err(ArgvWordsError.Rejected(kind: "argv-words", message: message))
}

# A byte that cannot occur inside any parsed word, used to join the words so a
# single `split` recovers the list.
#
# A word keeps only bytes the input already contained, so a byte absent from
# the input is absent from every word and the joined form splits back exactly.
# NUL is the usual choice; the remaining control bytes cover input that
# contains it, and once every candidate occurs, repeating one of them outruns
# every run the input can hold, so the search always ends.
pure word_separator(text: Str) -> Str {
  let candidates = ["\0", "\x01", "\x02", "\x03"]
  var candidate_index = 0
  while candidate_index < candidates.len() {
    if !text.contains(candidates[candidate_index]) {
      return candidates[candidate_index]
    }
    candidate_index = candidate_index + 1
  }
  var wide_separator = "\x01\x01"
  while text.contains(wide_separator) {
    wide_separator = wide_separator + "\x01"
  }
  return wide_separator
}

## Split an argument string into the words a shell would pass as argv.
##
## Whitespace separates words. Single quotes take their content literally up to
## the closing quote, and double quotes take their content literally except
## that `\` escapes the next character while `$` and `` ` `` are rejected.
## Outside quotes `\` escapes the next character, and escaping a shell syntax
## character there is rejected like an unquoted one. The rejected set is
## `| < > ; & $ ` * ? [ ] ( ) { }`; a quoted or escaped member of it keeps its
## literal meaning, so `'*'` is the word `*`.
##
## Quotes concatenate (`'a'b"c"` is one word `abc`) and explicit empty quotes
## keep an empty word, so `''` is one empty word. Input that ends inside a
## quote, or with a trailing escape, is rejected. Rejections are `Err` values
## of kind `argv-words`.
export pure argv_words(text: Str) -> Result[List[Str]] {
  # One class byte per ASCII byte value (0-127). Both loops read it instead of
  # testing each character against the whole shell syntax set, which is the
  # difference between a handful of instructions and a long comparison chain
  # for every byte of the input:
  #   `.`  an ordinary byte, copied into the word verbatim
  #   `w`  whitespace (9-13, 32), which ends a word outside quotes
  #   `q`  a single quote, which opens a literal run
  #   `Q`  a double quote, which opens a quoted run
  #   `b`  a backslash, which escapes the next character
  #   `s`  a shell syntax character (38, 40-42, 59-60, 62-63, 91, 93, 123-125)
  #   `d`  `$` and `` ` `` (36, 96), rejected outside quotes and in double
  #        quotes, where the other shell syntax characters are literal
  let classes = ".........wwwww.................."
    + "w.Q.d.sqsss................ss.ss"
    + "...........................sbs.."
    + "d..........................sss.."

  # The class bytes of `classes`, bound by name for the comparisons below.
  let ordinary_class = ".".byte_at(0, 0)
  let whitespace_class = "w".byte_at(0, 0)
  let single_quote_class = "q".byte_at(0, 0)
  let double_quote_class = "Q".byte_at(0, 0)
  let escape_class = "b".byte_at(0, 0)
  let syntax_class = "s".byte_at(0, 0)
  let expansion_class = "d".byte_at(0, 0)

  let separator = word_separator(text)
  let length = text.byte_len()
  # Words accumulate into bounded chunks rather than one growing string: a
  # string append copies the string it extends, so a single accumulator would
  # copy the whole output once per word. The chunk boundary needs no
  # bookkeeping: the separator is written before every word but the first, so
  # plain concatenation of the chunks rebuilds the exact joined text.
  let chunk_limit = 4096
  var chunks: List[Str] = []
  var pending = ""
  var words = 0
  var index = 0
  while index < length {
    # Skip the whitespace between words.
    let lead = text.byte_at(index, -1)
    if lead > 127 {
      let skip_width = whitespace_width(text, index)
      if skip_width > 0 {
        index = index + skip_width
        continue
      }
    } else if classes.byte_at(lead, 0) == whitespace_class {
      index = index + 1
      continue
    }

    # Consume one word. `verbatim_start` marks the start of the byte run that is
    # copied into `word` verbatim and has not been appended yet, so a word costs
    # one slice per quote, escape, or syntax boundary rather than one per byte.
    var word = ""
    var verbatim_start = index
    while index < length {
      let current = text.byte_at(index, -1)
      if current > 127 {
        # A non-ASCII character continues the word unless it is whitespace.
        if whitespace_width(text, index) > 0 {
          break
        }
        index = index + character_width(text, index)
        continue
      }
      let current_class = classes.byte_at(current, 0)
      if current_class == ordinary_class {
        index = index + 1
        continue
      }
      if current_class == whitespace_class {
        break
      }
      if current_class == single_quote_class {
        # A single-quoted run is literal until its closing quote.
        word = word + text.byte_slice(verbatim_start, index - verbatim_start)
        index = index + 1
        let quoted_start = index
        while index < length and text.byte_at(index, -1) != 39 {
          index = index + 1
        }
        if index >= length {
          return rejected("unterminated single quote")
        }
        word = word + text.byte_slice(quoted_start, index - quoted_start)
        index = index + 1
        verbatim_start = index
        continue
      }
      if current_class == double_quote_class {
        # A double-quoted run is literal, except that `\` escapes the next
        # character and `$` or `` ` `` are rejected.
        word = word + text.byte_slice(verbatim_start, index - verbatim_start)
        index = index + 1
        var literal_start = index
        var closed = false
        while index < length {
          let quoted = text.byte_at(index, -1)
          if quoted > 127 {
            index = index + character_width(text, index)
            continue
          }
          let quoted_class = classes.byte_at(quoted, 0)
          if quoted_class == double_quote_class {
            word = word + text.byte_slice(literal_start, index - literal_start)
            index = index + 1
            closed = true
            break
          }
          if quoted_class == escape_class {
            # The escaped character is never checked against the shell syntax
            # set inside double quotes.
            word = word + text.byte_slice(literal_start, index - literal_start)
            index = index + 1
            if index >= length {
              return rejected("trailing escape")
            }
            let quoted_width = character_width(text, index)
            word = word + text.byte_slice(index, quoted_width)
            index = index + quoted_width
            literal_start = index
            continue
          }
          if quoted_class == expansion_class {
            return rejected(syntax_message(text, index))
          }
          index = index + character_width(text, index)
        }
        if !closed {
          return rejected("unterminated double quote")
        }
        verbatim_start = index
        continue
      }
      if current_class == escape_class {
        # Outside quotes `\` escapes the next character, and escaping a shell
        # syntax character is rejected like an unquoted one.
        word = word + text.byte_slice(verbatim_start, index - verbatim_start)
        index = index + 1
        if index >= length {
          return rejected("trailing escape")
        }
        let escaped = text.byte_at(index, -1)
        if escaped <= 127 {
          let escaped_class = classes.byte_at(escaped, 0)
          if escaped_class == syntax_class or escaped_class == expansion_class {
            return rejected(syntax_message(text, index))
          }
        }
        let escaped_width = character_width(text, index)
        word = word + text.byte_slice(index, escaped_width)
        index = index + escaped_width
        verbatim_start = index
        continue
      }
      # Every remaining class is a shell syntax character or a rejection in
      # double quotes, both rejected here.
      return rejected(syntax_message(text, index))
    }
    word = word + text.byte_slice(verbatim_start, index - verbatim_start)
    if words > 0 {
      pending = pending + separator
    }
    pending = pending + word
    words = words + 1
    if pending.byte_len() >= chunk_limit {
      chunks = chunks.push(pending)
      pending = ""
    }
  }
  if words == 0 {
    let no_words: List[Str] = []
    return Ok(no_words)
  }
  chunks = chunks.push(pending)
  var joined = ""
  var chunk_index = 0
  while chunk_index < chunks.len() {
    joined = joined + chunks[chunk_index]
    chunk_index = chunk_index + 1
  }
  return Ok(joined.split(separator, -1))
}
