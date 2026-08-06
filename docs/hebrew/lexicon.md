# The Ozen Hebrew Lexicon

Compiled 2026-08-06 (Asia/Jerusalem). Machine-readable companion: [`lexicon.json`](./lexicon.json).

**304 entries**: 98 slang and discourse markers, 177 Hebrew-transliterated programming terms,
29 morphology traps. **30 of them are backed by an observed failure in Ozen's own live log**, not
by intuition.

This document explains what is in the JSON, why each class exists, and which claims are sourced
versus which are the author's own judgement. The JSON is the deliverable; this is the manual.

---

## 1. Why a lexicon at all, when there is already an LLM

Ozen already sends every utterance to DictaLM 3.0. The obvious objection is that a 12B Hebrew-native
model should not need a word list. The live log says otherwise, and the strongest single piece of
evidence is this one word:

> **הרנס** (harness) appeared four times in one 70-utterance session and came out four different
> ways: `harness`, `the arnes`, `the earns`, and `the most robust RMS`.

The same speaker, the same word, the same model, the same session. A model that renders a term four
ways in one sitting has no representation of that term; it is guessing from context each time. That
is precisely the gap a locked glossary fills, and it is why the entry earns its keep.

The second reason is subtler and worse. Consider:

> **ריסט** (reset). The transcript already contained the correct Latin string `reset`. The output was
> **"we will rust the session"**.

The model did not fail to transliterate. It **corrupted a token that was already correct**. No
transliteration table fixes that; only an explicit "these strings pass through verbatim" constraint
does. That is a different mechanism from a glossary and both are needed.

---

## 2. Class 1: slang and discourse markers (98 entries)

### The failure shape

Israeli spoken Hebrew leans heavily on Arabic-, Yiddish- and Persian-derived particles that carry
**pragmatic** rather than propositional content. A literal engine translates the proposition and
drops the pragmatics, or worse, translates a frozen idiom word by word.

| Hebrew | Literal engine | Interpreter | Why |
|---|---|---|---|
| על הפנים | "on the face" | "it's in bad shape" | Frozen idiom; the literal is meaningless English |
| חבל על הזמן | "a pity about the time" | "amazing" *or* "not worth it" | Polarity flips by context |
| פצצה | "bomb" | "brilliant" | Literal reading is alarming in an ops context |
| נשבר לי | "it broke to me" | "I'm fed up" | Dative-experiencer; ungrammatical word-for-word |
| פרוטקציה | "protection" | "favouritism, pulling strings" | False friend, dangerous in a security context |
| סוף הדרך | "the end of the road" | "the absolute best" | Positive superlative despite an ominous literal |
| כאילו | "as if" | *(delete)* | Discourse filler; translating it inverts the clause |

### The three sub-shapes worth naming

**(a) Polysemy that only intonation resolves.** וואלה is surprise, confirmation, or reluctant
agreement depending entirely on prosody. The ASR discards prosody. **A text-only pipeline therefore
cannot disambiguate it, and no amount of prompt engineering changes that.** The honest policy is to
drop it, and to say out loud that dropping it loses information.

**(b) Markers that CANCEL the preceding clause.** These are the dangerous ones, because deleting them
as filler inverts the utterance:
- **בעצם** ("actually, come to think of it") retracts what was just said. The live log has
  `בנוסף לזה, צריך לסדר את העניין של... לא, בעצם אין איזה עניין שצריך לסדר` rendered correctly as
  "well, actually there isn't any particular issue" - a success worth pinning as a regression test.
- **סתם** in its retraction sense ("never mind, I was kidding") does the same.
- **דווקא** is a scope-and-contrast operator with three documented senses
  ([he.wiktionary](https://he.wiktionary.org/wiki/דווקא), fetched 2026-08-06). A fixed gloss is wrong
  in a large fraction of occurrences.

**(c) Idioms that are positive despite a negative literal.** חבל על הזמן, סוף הדרך, מטורף, שיגעון.
he.wiktionary confirms חבל על הזמן carries **both** a slang positive sense and an archaic negative
one, which makes it the single hardest item in this lexicon.

### Sourcing

Fetched individually from he.wiktionary on 2026-08-06 and quoted in the JSON `notes`: תכלס, סבבה,
חבל על הזמן, יאללה, אחלה, בלגן, אשכרה, פשלה, דווקא, חפיף, פדיחה, פצצה, כאילו.

**Three could not be verified**: the he.wiktionary pages for וואלה, לפרגן and פרגון return HTTP 404.
They are marked `not individually fetched` in the JSON rather than dropped, because מפרגן/פרגון is a
genuinely untranslatable concept (praise given without envy) that a Hebrew lexicon cannot omit.

Broader slang is attributed to Rosenthal's *מילון הסלנג המקיף* (Keter 2005, ~10,000 entries,
[National Library of Israel record](https://www.nli.org.il/en/books/NNL_ALEPH990024940280205171/NLI),
verified 2026-08-06). It is citable but **not machine-readable and not licensed for reuse**, so it
grounds claims and cannot be ingested.

Entries carrying `author knowledge, NOT source-verified` are exactly that. They are marked instead of
deleted because a lexicon that admits its weak entries is more useful than a short one that hides
them, but they are not evidence.

### The resource reality, stated plainly

- **he.wiktionary has 25,175 content pages and 53 active editors in the last 30 days**
  ([Special:Statistics](https://he.wiktionary.org/wiki/Special:Statistics), verified 2026-08-06).
  It is thinly maintained. Do not build a slang layer that depends on it.
- **The Academy of the Hebrew Language terms bank holds ~120,000 terms in ~240 dictionaries**, each a
  Hebrew term paired with an English one, organised into ~90 topics
  ([about page](https://terms.hebrew-academy.org.il/Home/About), read 2026-08-06 through the browser
  bridge because the domain returns HTTP 403 to crawlers). **No public API or bulk download is
  documented.**
- **Critically, the Academy is the wrong direction for Ozen.** It is prescriptive: it will tell you
  the approved Hebrew for *compiler* is מהדר. Orel says קומפיילר. What the Academy *is* good for is
  the register axis, because its records explicitly mark obsolete terms and list non-standard
  alternatives (foreign word, common-public word, slang word) under each entry. That structure is a
  ready-made formal-versus-spoken map, which is a different and more useful thing than a word list.
- **No Hebrew slang corpus, no Hebrew-English code-switching corpus, and no Hebrew-English parallel
  corpus is listed in the Israeli national resource index**
  ([NNLP-IL/Hebrew-Resources](https://github.com/NNLP-IL/Hebrew-Resources), verified 2026-08-06).
  That gap is real, and Ozen's self-building dictionary is already filling a corner of it.

---

## 3. Class 2: transliterated programming vocabulary (177 entries)

### What the entries carry

Every entry has the Hebrew-script form ASR actually produces, the Latin target, observed spelling
variants, and a generated `forms` block:

```json
"forms": {
  "prefixed_hebrew_script": ["הבילד","לבילד","בבילד","מבילד","ובילד","שבילד","כבילד"],
  "prefixed_latin_stem":    ["ה-build","ל-build","ב-build","מ-build","ו-build","ש-build","כ-build"],
  "plural_he": "בילדים",
  "prefixed_plural": ["הבילדים", "..."],
  "verbalised": "לבנות"
}
```

The `prefixed_latin_stem` list matters more than it looks. It is the shape that appears constantly in
the live log (`ה-build`, `ל-Names`, `ב-instructural buttons`, `מ-GPT`) and it is the shape that breaks
Ozen's current script router. See [`code-switching.md`](./code-switching.md).

### The 30 observed failures

These are not hypotheticals. Each is a transcript-and-output pair from
`~/Library/Application Support/ai.orellius.ozen/log.json`, read 2026-08-06.

| Hebrew heard | Meant | Ozen produced | Class |
|---|---|---|---|
| אפליי / פליי (x4) | apply | **"play"**, "make a play", "apply a layout" | Highest-frequency failure in the corpus |
| הרנס / הארנס / ארנס (x4) | harness | **"arnes"**, "earns", "RMS", "harness" | Unstable: 4 renderings, 1 session |
| ריסט | reset | **"rust"** | Latin token already correct, corrupted anyway |
| ריסטייל | restyle | **"reset"** | Meaning inverted: design change to destructive act |
| לורד / לואוד | load | **"lord"**, "loud" | No /w/ in Hebrew; load/lord/loud collapse |
| דימיין | daemon | **"imagine"** | Homograph with the real Hebrew word "imagined" |
| לפלנט | to the plan | **"planet"** | One clitic letter turned a plan into a planet |
| בטרמינלוד | in the terminal | **"terminally ill"** | Clitic fused into loanword |
| כדיילי דרייבר | as a daily driver | **"as a driver"** | Clitic כ- absorbed "daily" |
| הטול | the tool | **"the toll"** | Unwritten vowel |
| רוב לוקסים | Roblox | **"most luxes"** | Proper noun split into two Hebrew words |
| המורכב-דיזיין | mockup design | **"complex-design"** | Mishearing produced grammatical output |
| הריפר | reaper | **"repeater"** | |
| האי-סטופ | E-stop | **"anti-stop"** | Safety term, meaning inverted |
| הנאזל | nozzle | **"nasal"** | Game-domain noun |
| רפיולינג | refueling | **"repainting"** | Game-domain noun |
| הסטרדסט | stardust | **"host"** | In-game currency name |
| סארקל | circle | **"sparkle"** | |
| painים | panes | **"pains"** | Latin stem, Hebrew plural, one token |
| ה-Requestrior | orchestrator | **"Requestrior"** | Nonce word survived to the paste, twice |
| אסיין | assign | **"asin"** | Nonce word survived to the paste |
| פרסיסטה | persist | **"Persistah"** | Capitalised as if a proper noun |
| הלוגינג סקרין | login screen | **"logging screen"** | One unstressed vowel |
| פסקי | passkey | **"breaks"** | Wrong reading is grammatical Hebrew |
| אינטיקייט | authenticate | **"intuitive"** | Same utterance as above; two collapses, fluent output |
| concent | consent | **"concentration"** | Latin token expanded wrongly |
| היקאפס | hiccups | **"their space"** | In a complaint *about* hiccups |
| סיכוונצ'לי | sequentially | **(deleted)** | Silent omission |
| הקונטקסטואליטי | context window | "contextuality" | Register failure, not lexical |
| פרפורציונלי בעליל | blatantly disproportionate | **"not overly performative"** | Intensifier absorbed |

### The four patterns behind them

1. **Unwritten vowels.** Modern Hebrew is written without niqqud, so `tool/toll`, `load/lord/loud`,
   `login/logging` all transliterate to the same consonant skeleton. The distinguishing vowel is
   simply not in the text. DICTA's [Nakdan](https://nakdanpro.dicta.org.il/) exists for exactly this
   problem (verified 2026-08-06).

2. **The wrong reading is grammatical.** פסקי is a real Hebrew construct form ("verses of"). דימיין
   is a real Hebrew word ("imagined"). המורכב is a real Hebrew word ("the complex"). The mishearing
   therefore produces a well-formed English sentence, and **nothing downstream can see it**. This is
   the same defect class Ozen's `phonetics.rs` was built for, documented in its own header comment.

3. **Clitics eat the stem.** לפלנט, בטרמינלוד, כדיילי. One prefix letter and the token no longer
   matches anything. See [section 4](#4-class-3-morphology-traps-29-entries).

4. **The seeded vocabulary is dev-only, and the work is not.** `SEED_VOCAB` in `store.rs` holds 78
   software terms. Roughly half the live log is **game design** (nozzle, refueling, canopy, plot,
   currency, stardust, E-stop, taxi, NPC, holograms), and that is where the failures cluster. The
   seed list is measurably aimed at the wrong half of the corpus.

### The learned glossary is currently making things worse

Read from `~/Library/Application Support/ai.orellius.ozen/dictionary.json`, 2026-08-06:

- **52 promoted terms. Only 3 are technical** (טרמינל, הפלוט, הכסף). The other 49 are high-frequency
  **function words**: רוצה, צריך, תעשה, חושב, יותר, הכל, במקום, עצמו.
- **At least 6 are outright wrong**: `לעשות -> system`, `בעברית -> write`, `שנוכל -> install`,
  `עושה -> doesn't`, `שזה -> their`, `לזה -> addition`.
- These wrong entries are injected into the prompt as **"Preferred English renderings for terms this
  speaker uses"**, and one of them demonstrably corrupted output:

  > Input: `אפשר לעשות... להשתמש ב-instructural buttons`
  > Output: **"we can system... use the instructional buttons"**

  The forced hint `לעשות = system` put the word "system" into the sentence. That is a measured,
  causal harm from the aligner, in the live log, with `hints_used: 3` on that entry.

- **`mishearings: 0`, `exemplars: 0`, and `auto_fixed` totals 0 across all 70 utterances.** The
  supervised half of the learning system has never fired, because there are **0 corrections in 70
  utterances**. The self-correcting layer is, in practice, dead.
- The aligner also keys on Latin tokens: `system -> operation` is a promoted term whose `he` field
  contains an English word.
- `בנוסף לזה` (7 occurrences, the most frequent phrase in the corpus) produced **two separate
  single-word hints**, `בנוסף -> addition` and `לזה -> addition`, because a two-word idiom was aligned
  as two independent words.

**The fix is a filter, not a rewrite.** The aligner's Dice-plus-margin logic is sound and its tests
are good. What it lacks is a gate on *what kind of word* may be promoted. See
[`interpretation-quality.md` section 6](./interpretation-quality.md).

---

## 4. Class 3: morphology traps (29 entries)

These are why naive dictionary lookup fails on real Hebrew, and they are the reason a glossary must
match on **segmented** forms rather than surface strings.

### Clitics

UD Hebrew explicitly separates seven prefix clitics into distinct tokens: the oblique case markers
**ב ל כ מ**, the conjunction **ו**, the definite determiner **ה**, and the subordination marker **ש**
([UD Hebrew](https://universaldependencies.org/he/index.html), verified 2026-08-06). They stack:
`וכשה-build` is four morphemes in one whitespace token.

**The scale is not an edge case.** UD Hebrew-HTB contains 6,143 sentences, 114,648 tokens and
**160,195 syntactic words**, with 36,783 multiword tokens averaging 2.24 words each
([UD_Hebrew-HTB](https://universaldependencies.org/treebanks/he_htb/index.html), verified 2026-08-06).
Roughly **one in three whitespace tokens carries more than one word**. Any design that treats a
whitespace token as a word is wrong a third of the time.

### The covert definite article

UD documents this verbatim: *"the definite marker ה, when appearing after the case markers ב or ל, is
covert."* So **בריפו** is "in a repo" or "in the repo" with **no orthographic difference whatsoever**.
English forces a choice the Hebrew never made. No glossary and no phrase table fixes this; only
discourse context can, and often not even that. It is worth knowing that some of Ozen's article
choices are unavoidable coin flips.

### Construct state (סמיכות)

`פאנל הפאמפ` = "the pump panel". Three things break at once:
- The definiteness marker sits on the **second** noun but scopes over the whole phrase.
- Word order is head-first, the reverse of English compounds.
- The head noun changes form in the plural: קבצים becomes קבצי.

That last point is the practical one. **A glossary keyed on קבצים will never match קבצי.**

### Loanwords take native morphology

Borrowed roots are inflected in the pi'el pattern with full native conjugation: `לפקסס` (to fix),
`לדבג` (to debug), `למרג'ג` (to merge), `לגגל` (to google), and their inflected forms `פיקססתי`,
`מדבג`, `ימרג'ג`. **These share no surface substring with the English source.** No amount of
transliteration matching finds them. This is the single strongest argument for morphological
segmentation over string matching.

Plurals go both ways: `בנצ'מארקס` (English -s) appears in the live log alongside native `-ים`/`-ות`
forms. A lemmatiser must accept both.

### Tooling that exists

- **HebPipe** reports Token F1 99.95 but **Word F1 99.11** on UD Hebrew-HTB
  ([repo](https://github.com/amir-zeldes/HebPipe), verified 2026-08-06). The gap between those two
  numbers **is** the clitic-splitting problem, and 99.11 is good enough to build on.
- **RFTokenizer** is a dedicated morphological segmenter for the same job.
- **YAP** does joint morphological analysis and dependency parsing
  ([repo](https://github.com/OnlpLab/yap), verified 2026-08-06), **but its MILA lexicon requires
  separate licensing for production use.** That is a real constraint on shipping it, and it is the
  kind of thing that is cheaper to know now than after integration.

### Glossaries fail silently

Google Cloud Translation's glossary documentation states that individual terms over 1024 UTF-8 bytes
are **silently ignored**, and that some terms are treated as stopwords and skipped
([docs](https://docs.cloud.google.com/translate/docs/advanced/glossary), verified 2026-08-06).
DeepL's glossary docs are silent on when an entry does or does not fire
([docs](https://developers.deepl.com/docs/api-reference/glossaries), verified 2026-08-06).

This is the SCAR-004 shape exactly: a glossary that is not applied looks identical to one that is.
**Any glossary layer Ozen builds needs a discriminating test** - feed it the term and see the forced
rendering appear, then feed it the neighbouring term and see it stay quiet. Both directions, or the
glossary is a data file with no proof it does anything.

---

## 5. How to consume the JSON

```
lexicon.json
├── counts                 { slang: 98, dev-transliteration: 177, morphology: 29, total: 304 }
├── provenance             live corpus, code files read, external sources with dates
├── honesty                what is verified, what is not, known gaps
└── entries[]
    ├── id                 "slang:יאללה" | "dev:harness" | "morph:covert-definite"
    ├── class              slang | dev-transliteration | morphology
    ├── he                 Hebrew surface form
    ├── en                 [candidate renderings]
    ├── register           slang | colloquial | neutral | vulgar | dev | grammar
    ├── literal_mt         (slang) what a word-for-word engine emits
    ├── interpreter        (slang) what an interpreter emits
    ├── latin / variants   (dev) Latin target and observed ASR spellings
    ├── forms              (dev) generated prefixed, plural and verbalised forms
    ├── observed_failure   (dev) what Ozen actually produced, when observed
    ├── notes              why this entry is hard
    └── source             URL + verification date, or an explicit not-verified marker
```

**Suggested load order for Ozen:**
1. `class: dev-transliteration` with an `observed_failure` (30 entries) go in as **locked glossary
   terms** immediately. They are the ones with proof.
2. The remaining `dev-transliteration` entries seed `SEED_VOCAB` in `store.rs`, replacing the
   current 78-term list, which is measurably aimed at the wrong half of the corpus.
3. `class: slang` entries with a `literal_mt` that differs sharply from `interpreter` belong in the
   **prompt**, as a short idiom table, not in the glossary. They are phrase-level and context-gated.
4. `class: morphology` entries are **not** runtime data. They are the specification for how the
   glossary must match, and the checklist for reviewing any future matching code.

---

## 6. What this lexicon does not do

- **It is not a translation memory.** It fixes terms, not sentences. Sentence-level quality is
  [`interpretation-quality.md`](./interpretation-quality.md).
- **It does not solve the covert definite article**, because nothing does.
- **It cannot disambiguate intonation-carried particles** (וואלה, and the sarcastic reading of
  anything), because prosody is discarded before the text exists.
- **It is a snapshot of one speaker's register**, drawn from 70 utterances. Israeli slang moves fast
  and the dev vocabulary moves faster. Every entry carries a date for that reason.
- **The Hebrew spellings are the ones ASR produces, not the ones a person would write.** Those are
  different things, and only the first matters for matching Ozen's transcripts.

---

## Related

- [`code-switching.md`](./code-switching.md) - telling Hebrew from English inside one sentence
- [`interpretation-quality.md`](./interpretation-quality.md) - what UN-grade means and how to measure it
- [`product.md`](./product.md) - who already serves this need, and whether there is a wedge
