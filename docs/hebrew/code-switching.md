# Telling Hebrew from English Inside One Sentence

Written 2026-08-06 (Asia/Jerusalem). Evidence base: `~/Library/Application Support/ai.orellius.ozen/log.json`,
read live during this session. **It grew from 70 to 85 utterances while this was being written**, which
is itself the point: this is production data from daily use, not a test set.

---

## 1. The measurement first

| Quantity | Value |
|---|---|
| Utterances | 85 |
| Transcripts containing **both** Hebrew and Latin characters | **50 (59%)** |
| Transcripts that are pure Latin script | **0** |
| Transcripts that are pure Hebrew script | 35 (41%) |
| Whitespace tokens total | 2,565 |
| Pure-Latin tokens | 90 (3.5%) |
| Tokens of the shape `HEBREW-CLITIC + hyphen + LATIN-STEM` (`ה-build`) | **55** |
| Times `is_latin_script` returned `true` | **1** |
| Of those, times it was **correct** | **0** |

Three conclusions follow immediately, and none of them require any theory.

**(a) Code-switching is the normal case, not the edge case.** 59% of utterances mix scripts. A router
that picks one language for the whole clip is structurally wrong for the majority of the corpus.

**(b) The English content is sparse but load-bearing.** Only 3.5% of tokens are Latin, and they are
almost entirely the technical nouns the sentence is *about*: `profile service`, `Operation System`,
`Don't Trust the Client`, `end-to-end`, `implemented`, `smooth ride`. Losing them loses the point of
the sentence, and they are exactly the tokens a Hebrew-first pipeline is least careful with.

**(c) The current router has never once been right when it fired.** `is_latin_script` returned `true`
exactly once in 85 utterances, on entry 27, and that firing was a misroute. Details in section 3.

---

## 2. What Ozen does today

`src-tauri/src/lib.rs`:

```rust
fn is_latin_script(text: &str) -> bool {
    let mut latin = 0usize;
    let mut hebrew = 0usize;
    for c in text.chars() {
        if c.is_ascii_alphabetic() { latin += 1; }
        else if ('\u{0590}'..='\u{05FF}').contains(&c) { hebrew += 1; }
    }
    latin > hebrew && latin > 0
}
```

Called once per clip, on the whole transcript, and the result selects one of three whole-clip routes:
`repair_english`, `to_english`, or `polish_hebrew`.

The code comment is honest about what it is doing: *"Latin-dominant wins, so a mostly-English sentence
carrying one Hebrew word still routes to the English repair."* The design intent is sound. The
implementation counts **characters**, and that is where it breaks.

---

## 3. Exactly where it breaks

### 3.1 A mostly-Hebrew sentence with a few Latin tech terms

This is the live misroute. Entry 27:

```
transcript:  תחשוב, שילוב בין AI Operation System ל-AD
counted:     19 Latin characters, 14 Hebrew characters  ->  is_latin_script = TRUE
route taken: repair_english   (mode "repair" in the log)
correct:     to_english       (this is a Hebrew sentence)
output:      "Think about the integration between an AI Operating System and AD."
```

The output happens to read fine, because `REPAIR_PROMPT` is permissive enough to absorb a Hebrew
sentence. That is luck, not correctness. The prompt it was given says *"You are an English
speech-to-text repair engine... only repair what the accent broke, never rephrase, summarise, or
**translate**."* The pipeline handed a translation job to a prompt that is explicitly forbidden to
translate.

**Why the character count inverted.** English words are simply longer in characters than Hebrew words,
because Hebrew does not write most vowels. `Operation System` is 15 characters carrying two words.
The Hebrew `שילוב בין` is 8 characters carrying two words. **Counting characters systematically
over-weights English by roughly the vowel ratio.** Counting words instead would have given
Hebrew 4, Latin 4 - still a tie, still not enough. See section 4 for what actually resolves it.

### 3.2 A mostly-English sentence with one Hebrew word

This case does not appear in the corpus (0 pure-Latin transcripts, and no Latin-majority-by-words
transcripts either), so the failure is **projected, not observed**, and it is stated as such. The
mechanism is the mirror of 3.1: one Hebrew content word in an English sentence is a handful of Hebrew
characters against dozens of Latin ones, so the count is right by accident. The failure only appears
when the Hebrew word is long or repeated. Given the observed corpus, this direction is a low-priority
risk for this user, and saying so is more useful than inventing an example.

### 3.3 Transliterated English written in Hebrew letters

**This is the case the current design cannot see at all, and it is the most common one.**

```
transcript:  , אני רוצה שתמצא לי את הארנס עם ה-quality הכי גבוה שאפשר
Latin count: "quality" = 7
Hebrew count: ~40
is_latin_script = FALSE   ->  routes to translate.  Correct route.
output:      "I want you to find me the arnes with the highest quality possible"
```

The route was right and the output was still wrong. `הארנס` is **English content in Hebrew script**.
By character count it is indistinguishable from Hebrew. It is Latin-count-zero and it is not Hebrew.

The corpus is full of this: הרנס (harness), דימיין (daemon), אסיין (assign), נאזל (nozzle), רפיולינג
(refueling), מוקאפ (mockup), אורקסטרטור (orchestrator), הלוגינג סקרין (login screen), אקסטינציה
(extension), אסטימטד טיים (estimated time). See [`lexicon.md` section 3](./lexicon.md#3-class-2-transliterated-programming-vocabulary-177-entries)
for 30 observed failures of exactly this shape.

**A script counter is the wrong instrument for this question by construction**, because the question
is not "which script" but "which language", and in transliteration those two answers disagree.

### 3.4 The token that is both scripts at once

Observed verbatim in the log:

```
painים        Latin stem "pain" + Hebrew plural suffix "ים"   ->  "panes"
ה-build       Hebrew clitic "ה" + hyphen + Latin stem
שפר-progress  Hebrew "שפר" (that per) + hyphen + Latin stem
מה-processes  Hebrew "מ" + "ה" + hyphen + Latin stem
```

55 tokens in the corpus have the clitic-plus-Latin-stem shape. `is_latin_script` scores each one on
**both sides of the same ledger**: the single Hebrew clitic character increments `hebrew`, the five or
six Latin stem characters increment `latin`. One token votes twice, in opposite directions, weighted
by nothing meaningful.

Worse, this is precisely backwards. `ה-build` is the strongest possible evidence that the sentence's
**grammar is Hebrew** - a Hebrew definite article does not attach to a stem inside an English frame -
and the counter reads it as 5-to-1 evidence for English.

### 3.5 The whisper language tag is not a usable fallback

The obvious repair is "just use whisper's detected language instead". It is not available:

> *"Language detection capability of this model has been degraded during training - it is intended for
> mostly-hebrew audio transcription. Language token should be explicitly set to Hebrew."*
> - [ivrit-ai/whisper-large-v3-turbo model card](https://huggingface.co/ivrit-ai/whisper-large-v3-turbo),
> verified 2026-08-06

Ozen uses `full_lang_id_from_state` from exactly this model, with `speech_lang: "auto"` as the default.
The publisher says the capability was degraded on purpose. The corpus agrees: **`lang` is `"he"` on all
85 entries**, including entry 27 which the script counter called English. The tag is a constant, and a
constant carries no information.

---

## 4. A better algorithm: label spans, decide by grammar, route per span

The design principle comes from the code-switching literature: a mixed utterance has a **matrix
language** that supplies the grammatical frame and an **embedded language** that supplies content
items. The frame lives in **function morphemes**, not in content words. Content words are what get
borrowed; function morphemes are what do not.

That single idea fixes every case in section 3, because in all of them the disputed tokens are
**content** and the undisputed evidence is **function morphology**.

### Stage 1: segment into spans, splitting mixed tokens

For each whitespace token, in this order:

1. **Maqaf split.** If the token matches `^[הולבמשכ]{1,3}-(.+)$` where the tail begins with a Latin
   letter, emit a `CLITIC` span for the prefix and a span for the Latin tail. (`ה-build`, `מה-processes`)
2. **Attached-clitic split.** If the token is Hebrew script and begins with one to three of
   `ו ש ה ב ל כ מ` followed by a stem of length >= 3, emit `CLITIC` spans and a stem span. Ambiguous:
   `מרג'` begins with `מ` and is not a clitic. Resolve by requiring the remaining stem to be a known
   word or transliteration; if it is not, do not split.
3. **Hebrew-suffix-on-Latin-stem split.** If the token is a Latin stem followed by `ים`/`ות`/`יות`,
   emit the stem plus a `MORPH` span. (`painים`)
4. Otherwise, classify the whole token by the script of its alphabetic characters.

### Stage 2: label each span

| Label | Test |
|---|---|
| `CLITIC` | a Hebrew function prefix split off in stage 1 |
| `HEB_FUNC` | Hebrew span in a closed list of function words (את, של, אני, זה, לא, יש, עם, אבל, גם, רק, כמו, מה, איך, כדי, הוא, היא, שזה, אם, כל, על, אל) |
| `TRANSLIT` | Hebrew span that matches a `dev-transliteration` entry in `lexicon.json`, after clitic stripping and `phonetics::key` normalisation |
| `HEB` | any other Hebrew span |
| `LAT_FUNC` | Latin span in a closed English function list (the, a, an, is, of, and, to, for, with, that, this, it) |
| `LAT` | any other Latin span |
| `SKIP` | digits, punctuation, symbols |

`TRANSLIT` is the label that does not exist today, and it is the one that matters.

### Stage 3: decide the matrix language

```
fn matrix_language(spans) -> Matrix {
    // Rule 1 is decisive on its own. A Hebrew clitic cannot attach to a stem
    // inside an English grammatical frame.
    if spans.any(CLITIC) { return Hebrew }

    let he = spans.count(HEB_FUNC)
    let en = spans.count(LAT_FUNC)

    // Rule 2: function morphemes vote. Content words do not vote at all.
    if he > en { return Hebrew }
    if en > he { return English }

    // Rule 3: tie. Fall back to content-word counts, where a TRANSLIT span
    // counts as ENGLISH content living in Hebrew script.
    let he_c = spans.count(HEB)
    let en_c = spans.count(LAT) + spans.count(TRANSLIT)
    if he_c > en_c { Hebrew } else if en_c > he_c { English } else { Hebrew }
}
```

The final tie-break is Hebrew because that is the base rate: 85 of 85 utterances in this corpus are
Hebrew-matrix. A default should agree with the data it will actually see.

**Applied to entry 27** (`תחשוב, שילוב בין AI Operation System ל-AD`):
Rule 1 fires on `ל-AD`. Matrix = Hebrew. Route = translate. **Correct**, and it did not even need
rules 2 or 3.

**Applied to `אני רוצה שתמצא לי את הארנס עם ה-quality הכי גבוה`:**
Rule 1 fires on `ה-quality`. Matrix = Hebrew, which the old code also got right. But stage 2 now
additionally labels `הארנס` as `TRANSLIT -> harness`, which the old code could not see at all.

### Stage 4: route per span, not per clip

The matrix language selects the prompt. The span labels then determine what the model is *allowed* to
touch:

- **`LAT` spans are protected.** They enter the prompt inside an explicit do-not-modify marker and are
  verified byte-identical on the way out. This is the direct fix for the worst class in the corpus:
  `reset` was already correct Latin text in the transcript and came out as **"rust"**; `concent` came
  out as **"concentration"**. Those are not translation failures, they are the model editing text it
  was never asked to edit.
- **`TRANSLIT` spans are normalised to Latin before the model sees them**, when the lexicon entry is
  unambiguous (הרנס, אנדפוינט, רפיולינג, נאזל). This is the fix for the harness class.
- **`TRANSLIT` spans whose Latin form is also a real Hebrew word are OFFERED, never applied.**
  דימיין (daemon / "imagined"), של (shell / "of"), פסקי (passkey / "verses of"), טול (tool / a real
  Hebrew noun). Only the sentence can settle these, and the model is the thing holding the sentence.
  **Ozen already has exactly this mechanism** - the `Suspect` type in `store.rs` and the "may be
  speech-recognition errors... substitute ONLY where the sentence clearly means the second" block in
  `translate.rs`. The work is to feed it from the lexicon, not to build it.
- **`CLITIC` spans are grammar and are dropped from the content stream**, having already done their
  job by voting in stage 3.

---

## 5. Failure modes the new algorithm still has

Stated up front, because a design that does not name its residual failures has not been thought
through.

**1. Homographic transliterations are unresolvable at the span level, permanently.**
דימיין is both "daemon" and the ordinary Hebrew word "imagined". No amount of span labelling picks
between them; only sentence semantics does. The algorithm's honest output here is a *suspect*, not a
decision. This is a real ceiling, not a tuning problem.

**2. Proper nouns split across tokens by the ASR.**
`רובלוקס` (Roblox) came back as `רוב לוקסים` - two well-formed Hebrew words meaning "most luxes", which
the model then translated faithfully. Span labelling operates on tokens the ASR produced; it cannot
rejoin a word the ASR split. Only a **phrase-level** lexicon lookup over n-grams catches this, and
only for names already in the lexicon.

**3. Clipped transliterations are genuinely ambiguous.**
`פליי` is both `play` and a clipped `apply`. Note the live evidence, which is sharp: the full form
`אפליי` was rendered **correctly** as "apply" in entry 83, while the clipped `פליי` failed **four
times**. The lexicon can map the clipped form, but that mapping is a judgement about this speaker,
not a derivation. It will be wrong for a speaker who actually means "play".

**4. The tie-break is a guess.**
Rules 1 and 2 are evidence. Rule 3 is a base-rate prior, and a truly balanced bilingual sentence with
no clitics and no function-word majority gets a coin flip dressed as a decision. Zero such utterances
exist in this corpus, so the risk is currently theoretical, but it is a guess and should be logged as
one so it can be measured later.

**5. Function-word lists are closed lists, and closed lists rot.**
The Hebrew function list above overlaps `content_tokens`'s existing `STOP` array in `store.rs`. They
should be **one list with two consumers**, or they will drift. `של` is on it, which is correct for the
aligner and wrong for the glossary, since `של` is also the transliteration of `shell`.

**6. Segmentation is being hand-rolled.**
Stage 1 is a hand-written approximation of a solved problem. HebPipe reports Word F1 **99.11** on UD
Hebrew-HTB against Token F1 99.95 ([repo](https://github.com/amir-zeldes/HebPipe), verified
2026-08-06), and the gap between those two numbers *is* clitic splitting. A regex will not reach 99.11.
The trade is a Python dependency and latency against accuracy, and it is a real trade, not an obvious
win. YAP is the other option but its MILA lexicon needs separate licensing for production
([repo](https://github.com/OnlpLab/yap), verified 2026-08-06), which is a shipping constraint worth
knowing before integration rather than after.

**7. There is no dataset to validate any of this against.**
No Hebrew-English code-switching corpus exists in LinCE, in the CALCS shared tasks, or in the Israeli
national resource index ([NNLP-IL/Hebrew-Resources](https://github.com/NNLP-IL/Hebrew-Resources),
verified 2026-08-06). The only ground truth available is Ozen's own log, hand-labelled. That is a real
limitation on any accuracy claim, and it is also the honest reason the evaluation set in
[`interpretation-quality.md`](./interpretation-quality.md) has to be built by hand.

---

## 6. Prior art, and how hard this problem actually is

- **Token-level language ID in code-switched text is an established task**, benchmarked by
  [LinCE](https://arxiv.org/abs/2005.04322) (Aguilar, Kar, Solorio, LREC 2020; 10 corpora, 4 language
  pairs, token-level LID among the tasks) and the CALCS shared-task series. Verified 2026-08-06.
  Neither covers Hebrew.
- **The closer framing is borrowing detection.** The ADoBo shared task treats "is this token a
  borrowing from the other language" as BIO sequence labelling
  ([task site](https://adobo-task.github.io/), verified 2026-08-06). Its
  [annotation guidelines](https://adobo-task.github.io/docs/guidelines.pdf) are the closest available
  public definition of the borrowing-versus-code-switch boundary, and worth adopting rather than
  reinventing.
- **Calibration on difficulty.** In the 2025 IberLEF edition of ADoBo, system F1 ranged from **0.17 to
  0.99** ([arXiv 2507.21813](https://arxiv.org/abs/2507.21813), verified 2026-08-06). That spread is
  the honest headline: this task is not solved, and system quality varies enormously. Any claim that
  Ozen's span labeller "handles code-switching" needs a number attached to it, measured on
  hand-labelled data.
- **The mirror-image problem is well studied.** Hindi transliterated into Latin script (Hinglish) has
  the two documented core challenges inverted from ours: non-standard transliterated spellings, and
  transliterations colliding with real words of the host language. Both are exactly what sections 3.3
  and 5.1 describe, which is mild evidence that the failure taxonomy here is the right one.

---

## 7. Recommended order of work

1. **Split mixed tokens and add rule 1 only** (a Hebrew clitic anywhere forces the Hebrew matrix).
   This is a small change to one function, it fixes the single observed misroute, and it cannot make
   any currently-correct clip worse - rule 1 only ever routes *toward* Hebrew, and 85 of 85 clips are
   Hebrew.
2. **Protect `LAT` spans through the model.** Highest value per line of code in this document: it
   fixes `reset -> rust` and `concent -> concentration`, which are failures on text that was already
   correct.
3. **Add the `TRANSLIT` label**, fed from `lexicon.json`, applying unambiguous entries silently and
   routing ambiguous ones into the existing `Suspect` path.
4. **Only then** consider real morphological segmentation, and only with a measured accuracy number
   to justify the dependency.

Before any of this is called done, it needs a discriminating test in both directions, per
`STRIATUM_REWARD.md`: feed the router entry 27 and see it now choose translate, **and** feed it a
genuine English clip and see it still choose repair. A router that always says Hebrew would pass the
first test alone, and would be indistinguishable from one that works.
