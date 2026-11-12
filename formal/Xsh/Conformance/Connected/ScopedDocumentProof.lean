import Xsh.Conformance.Connected.ScopedParseStates
namespace Xsh.Conformance.Connected
open Xsh.Conformance.Transport

def parserChunk1 : List Token := scopedLexemes1.reverse.map PlainLexeme.token
def parserChunk2 : List Token := scopedLexemes2.reverse.map PlainLexeme.token
def parserChunk3 : List Token := scopedLexemes3.reverse.map PlainLexeme.token
def parserChunk4 : List Token := scopedLexemes4.reverse.map PlainLexeme.token
def parserChunk5 : List Token := scopedLexemes5.reverse.map PlainLexeme.token
def parserChunk6 : List Token := scopedLexemes6.reverse.map PlainLexeme.token
def parserChunk7 : List Token := scopedLexemes7.reverse.map PlainLexeme.token
def parserChunk8 : List Token := scopedLexemes8.reverse.map PlainLexeme.token
def parserChunk9 : List Token := scopedLexemes9.reverse.map PlainLexeme.token
def parserChunk10 : List Token := scopedLexemes10.reverse.map PlainLexeme.token
def parserChunk11 : List Token := scopedLexemes11.reverse.map PlainLexeme.token
def parserChunk12 : List Token := scopedLexemes12.reverse.map PlainLexeme.token
def parserChunk13 : List Token := scopedLexemes13.reverse.map PlainLexeme.token
def parserChunk14 : List Token := scopedLexemes14.reverse.map PlainLexeme.token
def parserChunk15 : List Token := scopedLexemes15.reverse.map PlainLexeme.token
def parserChunk16 : List Token := scopedLexemes16.reverse.map PlainLexeme.token
def parserChunk17 : List Token := scopedLexemes17.reverse.map PlainLexeme.token
def parserChunk18 : List Token := scopedLexemes18.reverse.map PlainLexeme.token
def parserChunk19 : List Token := scopedLexemes19.reverse.map PlainLexeme.token
def parserChunk20 : List Token := scopedLexemes20.reverse.map PlainLexeme.token
def parserChunk21 : List Token := scopedLexemes21.reverse.map PlainLexeme.token
def parserChunk22 : List Token := scopedLexemes22.reverse.map PlainLexeme.token
def parserChunk23 : List Token := scopedLexemes23.reverse.map PlainLexeme.token
def parserChunk24 : List Token := scopedLexemes24.reverse.map PlainLexeme.token
def parserChunk25 : List Token := scopedLexemes25.reverse.map PlainLexeme.token
def parserChunk26 : List Token := scopedLexemes26.reverse.map PlainLexeme.token
def parserTail26 : List Token := parserChunk26
def parserTail25 : List Token := parserChunk25 ++ parserTail26
def parserTail24 : List Token := parserChunk24 ++ parserTail25
def parserTail23 : List Token := parserChunk23 ++ parserTail24
def parserTail22 : List Token := parserChunk22 ++ parserTail23
def parserTail21 : List Token := parserChunk21 ++ parserTail22
def parserTail20 : List Token := parserChunk20 ++ parserTail21
def parserTail19 : List Token := parserChunk19 ++ parserTail20
def parserTail18 : List Token := parserChunk18 ++ parserTail19
def parserTail17 : List Token := parserChunk17 ++ parserTail18
def parserTail16 : List Token := parserChunk16 ++ parserTail17
def parserTail15 : List Token := parserChunk15 ++ parserTail16
def parserTail14 : List Token := parserChunk14 ++ parserTail15
def parserTail13 : List Token := parserChunk13 ++ parserTail14
def parserTail12 : List Token := parserChunk12 ++ parserTail13
def parserTail11 : List Token := parserChunk11 ++ parserTail12
def parserTail10 : List Token := parserChunk10 ++ parserTail11
def parserTail9 : List Token := parserChunk9 ++ parserTail10
def parserTail8 : List Token := parserChunk8 ++ parserTail9
def parserTail7 : List Token := parserChunk7 ++ parserTail8
def parserTail6 : List Token := parserChunk6 ++ parserTail7
def parserTail5 : List Token := parserChunk5 ++ parserTail6
def parserTail4 : List Token := parserChunk4 ++ parserTail5
def parserTail3 : List Token := parserChunk3 ++ parserTail4
def parserTail2 : List Token := parserChunk2 ++ parserTail3
def parserTail1 : List Token := parserChunk1 ++ parserTail2

set_option maxRecDepth 100000 in
theorem scopedTokenChunks : scopedLexAcc26.reverse = parserTail1 := by
  simp only [scopedLexAcc1, parserTail1, parserChunk1, scopedLexAcc2, parserTail2, parserChunk2, scopedLexAcc3, parserTail3, parserChunk3, scopedLexAcc4, parserTail4, parserChunk4, scopedLexAcc5, parserTail5, parserChunk5, scopedLexAcc6, parserTail6, parserChunk6, scopedLexAcc7, parserTail7, parserChunk7, scopedLexAcc8, parserTail8, parserChunk8, scopedLexAcc9, parserTail9, parserChunk9, scopedLexAcc10, parserTail10, parserChunk10, scopedLexAcc11, parserTail11, parserChunk11, scopedLexAcc12, parserTail12, parserChunk12, scopedLexAcc13, parserTail13, parserChunk13, scopedLexAcc14, parserTail14, parserChunk14, scopedLexAcc15, parserTail15, parserChunk15, scopedLexAcc16, parserTail16, parserChunk16, scopedLexAcc17, parserTail17, parserChunk17, scopedLexAcc18, parserTail18, parserChunk18, scopedLexAcc19, parserTail19, parserChunk19, scopedLexAcc20, parserTail20, parserChunk20, scopedLexAcc21, parserTail21, parserChunk21, scopedLexAcc22, parserTail22, parserChunk22, scopedLexAcc23, parserTail23, parserChunk23, scopedLexAcc24, parserTail24, parserChunk24, scopedLexAcc25, parserTail25, parserChunk25, scopedLexAcc26, parserTail26, parserChunk26, List.reverse_append, List.reverse_reverse, List.append_assoc]

theorem parserTransition1 (suffix : List Token) : parseFrom parserState0 (parserChunk1 ++ suffix) = parseFrom parserState1 suffix := by
  simp only [parserChunk1, parseFrom_append, parserStep1]

theorem parserTransition2 (suffix : List Token) : parseFrom parserState1 (parserChunk2 ++ suffix) = parseFrom parserState2 suffix := by
  simp only [parserChunk2, parseFrom_append, parserStep2]

theorem parserTransition3 (suffix : List Token) : parseFrom parserState2 (parserChunk3 ++ suffix) = parseFrom parserState3 suffix := by
  simp only [parserChunk3, parseFrom_append, parserStep3]

theorem parserTransition4 (suffix : List Token) : parseFrom parserState3 (parserChunk4 ++ suffix) = parseFrom parserState4 suffix := by
  simp only [parserChunk4, parseFrom_append, parserStep4]

theorem parserTransition5 (suffix : List Token) : parseFrom parserState4 (parserChunk5 ++ suffix) = parseFrom parserState5 suffix := by
  simp only [parserChunk5, parseFrom_append, parserStep5]

theorem parserTransition6 (suffix : List Token) : parseFrom parserState5 (parserChunk6 ++ suffix) = parseFrom parserState6 suffix := by
  simp only [parserChunk6, parseFrom_append, parserStep6]

theorem parserTransition7 (suffix : List Token) : parseFrom parserState6 (parserChunk7 ++ suffix) = parseFrom parserState7 suffix := by
  simp only [parserChunk7, parseFrom_append, parserStep7]

theorem parserTransition8 (suffix : List Token) : parseFrom parserState7 (parserChunk8 ++ suffix) = parseFrom parserState8 suffix := by
  simp only [parserChunk8, parseFrom_append, parserStep8]

theorem parserTransition9 (suffix : List Token) : parseFrom parserState8 (parserChunk9 ++ suffix) = parseFrom parserState9 suffix := by
  simp only [parserChunk9, parseFrom_append, parserStep9]

theorem parserTransition10 (suffix : List Token) : parseFrom parserState9 (parserChunk10 ++ suffix) = parseFrom parserState10 suffix := by
  simp only [parserChunk10, parseFrom_append, parserStep10]

theorem parserTransition11 (suffix : List Token) : parseFrom parserState10 (parserChunk11 ++ suffix) = parseFrom parserState11 suffix := by
  simp only [parserChunk11, parseFrom_append, parserStep11]

theorem parserTransition12 (suffix : List Token) : parseFrom parserState11 (parserChunk12 ++ suffix) = parseFrom parserState12 suffix := by
  simp only [parserChunk12, parseFrom_append, parserStep12]

theorem parserTransition13 (suffix : List Token) : parseFrom parserState12 (parserChunk13 ++ suffix) = parseFrom parserState13 suffix := by
  simp only [parserChunk13, parseFrom_append, parserStep13]

theorem parserTransition14 (suffix : List Token) : parseFrom parserState13 (parserChunk14 ++ suffix) = parseFrom parserState14 suffix := by
  simp only [parserChunk14, parseFrom_append, parserStep14]

theorem parserTransition15 (suffix : List Token) : parseFrom parserState14 (parserChunk15 ++ suffix) = parseFrom parserState15 suffix := by
  simp only [parserChunk15, parseFrom_append, parserStep15]

theorem parserTransition16 (suffix : List Token) : parseFrom parserState15 (parserChunk16 ++ suffix) = parseFrom parserState16 suffix := by
  simp only [parserChunk16, parseFrom_append, parserStep16]

theorem parserTransition17 (suffix : List Token) : parseFrom parserState16 (parserChunk17 ++ suffix) = parseFrom parserState17 suffix := by
  simp only [parserChunk17, parseFrom_append, parserStep17]

theorem parserTransition18 (suffix : List Token) : parseFrom parserState17 (parserChunk18 ++ suffix) = parseFrom parserState18 suffix := by
  simp only [parserChunk18, parseFrom_append, parserStep18]

theorem parserTransition19 (suffix : List Token) : parseFrom parserState18 (parserChunk19 ++ suffix) = parseFrom parserState19 suffix := by
  simp only [parserChunk19, parseFrom_append, parserStep19]

theorem parserTransition20 (suffix : List Token) : parseFrom parserState19 (parserChunk20 ++ suffix) = parseFrom parserState20 suffix := by
  simp only [parserChunk20, parseFrom_append, parserStep20]

theorem parserTransition21 (suffix : List Token) : parseFrom parserState20 (parserChunk21 ++ suffix) = parseFrom parserState21 suffix := by
  simp only [parserChunk21, parseFrom_append, parserStep21]

theorem parserTransition22 (suffix : List Token) : parseFrom parserState21 (parserChunk22 ++ suffix) = parseFrom parserState22 suffix := by
  simp only [parserChunk22, parseFrom_append, parserStep22]

theorem parserTransition23 (suffix : List Token) : parseFrom parserState22 (parserChunk23 ++ suffix) = parseFrom parserState23 suffix := by
  simp only [parserChunk23, parseFrom_append, parserStep23]

theorem parserTransition24 (suffix : List Token) : parseFrom parserState23 (parserChunk24 ++ suffix) = parseFrom parserState24 suffix := by
  simp only [parserChunk24, parseFrom_append, parserStep24]

theorem parserTransition25 (suffix : List Token) : parseFrom parserState24 (parserChunk25 ++ suffix) = parseFrom parserState25 suffix := by
  simp only [parserChunk25, parseFrom_append, parserStep25]

theorem parserTransition26 (suffix : List Token) : parseFrom parserState25 (parserChunk26 ++ suffix) = parseFrom parserState26 suffix := by
  simp only [parserChunk26, parseFrom_append, parserStep26]

set_option maxRecDepth 100000 in
theorem parserGroup1 : parseFrom parserState0 parserTail1 = parseFrom parserState5 parserTail6 := by
  change parseFrom parserState0 (parserChunk1 ++ parserTail2) = _
  rw [parserTransition1]
  change parseFrom parserState1 (parserChunk2 ++ parserTail3) = _
  rw [parserTransition2]
  change parseFrom parserState2 (parserChunk3 ++ parserTail4) = _
  rw [parserTransition3]
  change parseFrom parserState3 (parserChunk4 ++ parserTail5) = _
  rw [parserTransition4]
  change parseFrom parserState4 (parserChunk5 ++ parserTail6) = _
  rw [parserTransition5]

set_option maxRecDepth 100000 in
theorem parserGroup6 : parseFrom parserState5 parserTail6 = parseFrom parserState10 parserTail11 := by
  change parseFrom parserState5 (parserChunk6 ++ parserTail7) = _
  rw [parserTransition6]
  change parseFrom parserState6 (parserChunk7 ++ parserTail8) = _
  rw [parserTransition7]
  change parseFrom parserState7 (parserChunk8 ++ parserTail9) = _
  rw [parserTransition8]
  change parseFrom parserState8 (parserChunk9 ++ parserTail10) = _
  rw [parserTransition9]
  change parseFrom parserState9 (parserChunk10 ++ parserTail11) = _
  rw [parserTransition10]

set_option maxRecDepth 100000 in
theorem parserGroup11 : parseFrom parserState10 parserTail11 = parseFrom parserState15 parserTail16 := by
  change parseFrom parserState10 (parserChunk11 ++ parserTail12) = _
  rw [parserTransition11]
  change parseFrom parserState11 (parserChunk12 ++ parserTail13) = _
  rw [parserTransition12]
  change parseFrom parserState12 (parserChunk13 ++ parserTail14) = _
  rw [parserTransition13]
  change parseFrom parserState13 (parserChunk14 ++ parserTail15) = _
  rw [parserTransition14]
  change parseFrom parserState14 (parserChunk15 ++ parserTail16) = _
  rw [parserTransition15]

set_option maxRecDepth 100000 in
theorem parserGroup16 : parseFrom parserState15 parserTail16 = parseFrom parserState20 parserTail21 := by
  change parseFrom parserState15 (parserChunk16 ++ parserTail17) = _
  rw [parserTransition16]
  change parseFrom parserState16 (parserChunk17 ++ parserTail18) = _
  rw [parserTransition17]
  change parseFrom parserState17 (parserChunk18 ++ parserTail19) = _
  rw [parserTransition18]
  change parseFrom parserState18 (parserChunk19 ++ parserTail20) = _
  rw [parserTransition19]
  change parseFrom parserState19 (parserChunk20 ++ parserTail21) = _
  rw [parserTransition20]

set_option maxRecDepth 100000 in
theorem parserGroup21 : parseFrom parserState20 parserTail21 = parseFrom parserState25 parserTail26 := by
  change parseFrom parserState20 (parserChunk21 ++ parserTail22) = _
  rw [parserTransition21]
  change parseFrom parserState21 (parserChunk22 ++ parserTail23) = _
  rw [parserTransition22]
  change parseFrom parserState22 (parserChunk23 ++ parserTail24) = _
  rw [parserTransition23]
  change parseFrom parserState23 (parserChunk24 ++ parserTail25) = _
  rw [parserTransition24]
  change parseFrom parserState24 (parserChunk25 ++ parserTail26) = _
  rw [parserTransition25]

set_option maxRecDepth 100000 in
theorem parserGroup26 : parseFrom parserState25 parserTail26 = parseFrom parserState26 [] := by
  change parseFrom parserState25 (parserChunk26 ++ []) = _
  rw [parserTransition26]

set_option maxRecDepth 100000 in
theorem parserFinished : parseFrom parserState26 [] = some scopedFactsWire := by rfl

theorem initial_parse (tokens : List Token) : parseTokens tokens [] (.value false) = parseFrom parserState0 tokens := by rfl

set_option maxRecDepth 100000 in
theorem parserFull : parseFrom parserState0 parserTail1 = some scopedFactsWire :=
  parserGroup1.trans (parserGroup6.trans (parserGroup11.trans (parserGroup16.trans
    (parserGroup21.trans (parserGroup26.trans parserFinished)))))

set_option maxRecDepth 100000 in
theorem scopedTokensParseFactored : parseTokens scopedLexAcc26.reverse [] (.value false) = some scopedFactsWire :=
  (initial_parse _).trans ((congrArg (parseFrom parserState0) scopedTokenChunks).trans parserFull)


set_option maxRecDepth 100000 in
theorem scoped_within_limit : ¬ maxDocumentBytes < scopedFactsCodes.length := by decide

theorem scoped_facts_parse_to_wire : parseCodes scopedFactsCodes = some scopedFactsWire :=
  parseCodes_from_parts scopedFactsCodes scopedLexAcc26.reverse scopedFactsWire scoped_within_limit scopedLexResult scopedTokensParseFactored

end Xsh.Conformance.Connected
