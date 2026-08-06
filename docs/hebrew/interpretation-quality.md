# UN-Grade Interpretation, Operationalized

Written 2026-08-06 (Asia/Jerusalem). The brief was: *"coherent translation the way they do it at the
UN council... coherent and written in such a way that I don't have to go back through the text and
fix it manually."*

This document turns that into a rubric, a set of pipeline changes, and a way to measure whether any
of it worked. Evidence base: 85 real utterances from
`~/Library/Application Support/ai.orellius.ozen/log.json`, read 2026-08-06.

---

## 1. What "UN-grade" actually is

Not a metaphor. The UN publishes its own criteria.

### The UN's own rubric

Candidates in the UN competitive examination for interpreters are assessed on
([un.org/dgacm/en/content/exams-interpreters](https://www.un.org/dgacm/en/content/exams-interpreters),
verified 2026-08-06):

- passive comprehension of the source languages
- **"accuracy in interpreting into the target language in a grammatically correct manner"**
- ability to **"construct complete sentences"**
- **"appropriate style and register"**
- capacity to **"keep up with the speed"**
- good diction and delivery

And on the job ([un.org/dgacm/en/content/interpretation](https://www.un.org/dgacm/en/content/interpretation),
verified 2026-08-06), interpreters *"must master the specific vocabulary (or jargon) of the
Organization"*, *"must be able to comprehend every imaginable accent"*, cope with *"issues of speed and
style"*, and *"find proper cultural equivalents and take cultural context into account"*.

Three of those map onto Ozen's observed failures with no interpretation needed:

| UN criterion | Ozen's observed failure |
|---|---|
| "construct complete sentences" | Run-on speech pasted as run-on English; 31 of 85 transcripts open with a stray comma |
| "master the specific vocabulary" | הרנס rendered four ways in one session |
| "appropriate style and register" | "your contextuality is low" instead of "your context window is running low" |

### The interpreting-studies rubric

Bühler (1986) listed 16 criteria; Kurz (1989) highlighted sense consistency, logical cohesion, correct
terminology, completeness, fluency, correct grammar; Gile added informational fidelity and correct
language. The field commonly groups these into **three categories: information fidelity, delivery, and
target-language quality**
([Humanities and Social Sciences Communications, 2024](https://www.nature.com/articles/s41599-024-03511-6),
verified 2026-08-06).

For a text-output system, "delivery" collapses to latency, which leaves **fidelity** and
**target-language quality** as the two axes that matter. They fail differently and must be measured
separately: Ozen's output is usually fluent and sometimes unfaithful, which is the worst combination
because fluency hides the infidelity.

### The theory, with its honest caveat

The interpretive theory of translation (Seleskovitch, ESIT) models interpreting as
**comprehension -> deverbalization -> reformulation**: the interpreter breaks from the surface form and
retains sense, then re-expresses it. That is a good design metaphor for what Ozen should do instead of
transcoding word by word.

**It is a metaphor, not evidence.** Deverbalization *"has never been adequately justified nor falsified
since its proposal by Danica Seleskovitch"* (Zhang Jiliang, *FORUM* 8(1):213-236, 2010,
[JBe Platform](https://www.jbe-platform.com/content/journals/10.1075/forum.8.1.09zha), verified
2026-08-06). It is cited here as a framing and nothing is built on it.

### Register, defined operationally

Halliday's definition is usable as three prompt inputs
([Glottopedia](http://glottopedia.org/index.php/Register_(discourse)), verified 2026-08-06):

- **FIELD** - the subject matter and purposive activity. For Ozen: is this utterance about code, game
  design, product, or admin?
- **TENOR** - the role relations between participants. Is this an order to an agent, a note to self,
  or a message to a person?
- **MODE** - the channel and rhetorical mode. Always the same here: **spoken, extempore, being
  typed into a text field.**

MODE is constant and is the source of most of the pain. Speech is not writing: it has false starts,
self-repair, no sentence boundaries, and hedges that exist to buy thinking time. Ozen's job is a MODE
conversion, spoken-extempore into written-instruction, and that is a bigger transformation than the
Hebrew-to-English part.

---

## 2. Before and after, on real utterances

All "before" columns are verbatim from the live log. "After" is what an interpreter renders.

| # | Hebrew (as transcribed) | Ozen produced | Interpreter would produce | Failure class |
|---|---|---|---|---|
| 11 | `בנוסף לזה, אני לא חושב שעדיין חיברנו את הסוכנים... יש חלונית שכתוב No guest attached` | **`בנוסף לזה, אני לא think שעדיין חיברנו את הסוכנים... No just attached`** | "Also, I don't think we've wired up the agents yet, if at all. One more small thing: there's a panel that says No guest attached, and clicking the attach button does nothing." | **Total route collapse.** See section 3 |
| 39 | `תעשה prompt ונעשה reset לסשן` | "make a prompt and we will **rust** the session" | "write a handoff prompt and let's reset the session" | Correct Latin token corrupted |
| 41 | `ריסטייל הפאמפ פאנל` | "**reset** the pump panel" | "restyle the pump panel" | Meaning inverted |
| 34 | `להשתמש בזה כדיילי דרייבר ולא בטרמינלוד` | "use this as a driver and not **terminally ill**" | "use this as my daily driver instead of the terminal" | Clitic fused into loanword |
| 60 | `תעשה רווייז לפלנט` | "do a revise to **planet**" | "revise the plan" | One clitic letter |
| 50 | `הארנס עם ה-quality הכי גבוה... הארנס שלנו על הארנס שלהם` | "the **arnes** with the highest quality... our **arnes** on their **arnes**" | "the highest-quality harness... our harness on top of theirs" | Unstable terminology |
| 64 | `בלי שזה יתפוס את כל ההיקאפס שלהם וכל הפאוסות` | "without taking up all **their space** and those pauses" | "without picking up all the hiccups and pauses" | Content word replaced |
| 14 | `הקטלוג הקיים של רוב לוקסים` | "the existing catalog of **most luxes**" | "Roblox's existing catalogue" | Proper noun split by ASR |
| 70 | `תפתח לי את המוקאפ בדפדפן של ברייף` | "open the mockup in a browser **for briefing**" | "open the mockup in Brave" | Proper noun lost |
| 59 | `סיכוונצ'לי מ-A עד E` | "the phases from A to E" *(adverb deleted)* | "the phases, sequentially, from A to E" | **Silent omission** |
| 56 | `צריך permission אבל ל-concent` | "permission is needed but for **concentration**" | "it needs permission, but for consent" | Latin token expanded wrongly |
| 37 | `שהקונטקסטואליטי שלך נמוך` | "that your **contextuality** is low" | "that you're running low on context" | Register, not lexis |
| 1 | `הנאזל לא יתחבר לרכב... לימיט לאורך של הנאזל פייפ` | "the **nasal** doesn't connect... the length of the **nasal** pipe" | "the nozzle shouldn't connect... a limit on the nozzle pipe's length" | Domain noun |
| 73 | `אם אין הופרס בין פלוט לפלוט... תפריט שאתה יכול לעשות ויזית` | "if there's no **handover** between plots... a menu you can make **visually**" | "if there are no doors between plots... a menu you can use to visit" | Two collapses, fluent output |
| 3, 9, 11, 15, ... | *31 of 85 transcripts begin with `, `* | 7 of those commas survive into the pasted English | *(strip before the model, as `trim_leading_noise` now does)* | Already fixed in v0.5.x |

Note the pattern in rows 64, 73 and 59: **the output is fluent, grammatical English every time.**
Nothing downstream can see the defect. That is the whole problem. A pipeline that is graded on fluency
will score these as successes.

---

## 3. Entry 11: the single most important data point in this corpus

```
transcript: , בנוסף לזה, אני לא חושב שעדיין חיברנו את הסוכנים, אם בכלל.
            וגם עוד משהו קטן, יש חלונית שכתוב No guest attached.
            ללחוץ על הכפתור של ה-attage לא עובד.

output:     , בנוסף לזה, אני לא think שעדיין חיברנו את הסוכנים, אם בכלל.
            וגם עוד משהו קטן, יש חלונית שכתוב No just attached.
            ללחוץ על הכפתור של ה-attage לא עובד.

log fields: mode "translate", hints_used 2, confidence 0.87
```

The model **did not translate**. It performed exactly two substitutions and returned the Hebrew.

The two substitutions are the tell. `חושב -> think` is a **promoted glossary term with 9 hits** sitting
in `dictionary.json` right now. It was injected into the prompt as *"Preferred English renderings for
terms this speaker uses"*, and the model applied it as a **find-and-replace instead of translating**.

That is a causal chain from the hint block to a catastrophic output, visible in the data:

> the hint table can convert the model from a translator into a substitution engine

`translate.rs` already anticipates the *adversarial* version of this risk - the comment on `hint_block`
explains that values are flattened and framed as a lookup table so that a past imperative cannot
re-enter as an instruction. The failure observed is the **benign** version of the same weakness: not
injection, but the table crowding out the task. It is worth adding to that comment, because the
mitigation is different (bound the table, not sanitise it).

**This one entry justifies three separate changes**, and each is cheap:
1. A deterministic post-check: *the English output must contain no Hebrew characters.* Would have
   caught this in microseconds (section 6).
2. A cap and a filter on the hint block (section 5.2).
3. A retry on oracle failure, with hints disabled on the second attempt.

---

## 4. What a single-shot DictaLM call structurally cannot do

Not a criticism of the model. These are things one forward pass cannot do by construction.

1. **It cannot know the register target.** `SYSTEM_PROMPT` says "keep it concise and imperative", which
   is right for a terminal command and wrong for the utterances in the log that are design musings or
   questions. One fixed register for all traffic guarantees a mismatch on some of it.
2. **It cannot verify its own terminology.** Nothing in a single pass compares the output's technical
   nouns against a glossary. הרנס came out four ways because nothing checked.
3. **It cannot notice that it corrupted a protected token.** `reset -> rust`, `concent -> concentration`.
   Verification requires comparing input to output, which requires a second look.
4. **It cannot re-segment run-on speech into complete sentences** while also translating, at
   temperature 0.2, reliably. "Construct complete sentences" is an explicit UN criterion and it is a
   distinct task from translating.
5. **It cannot detect its own omissions.** Dropping `סיקוונצ'לי` produced a *more* fluent sentence. A
   self-consistent model has no signal that anything is missing.

### The model choice is itself a lever, and the numbers are uncomfortable

DICTA's own technical report puts `DictaLM-3.0-Nemotron-12B-Instruct` - the exact model Ozen runs - at
**13.50** on the Translation benchmark, against **16.50** for `gemma-3-12b-it` at the same size and
**30.09** for DictaLM's own 24B-Thinking variant
([DictaLM 3.0 technical report](https://dicta.org.il/publications/DictaLM_3_0___Techincal_Report.pdf),
Table 8, verified 2026-08-06). The scores are **win-rates against Gemini 2.5 Pro judged by GPT-4o**, so
13.50 means it loses roughly 86% of the time.

**Two honest caveats, and they matter:** the benchmark direction is **English into Hebrew**, the
opposite of Ozen's; and it is a vendor-run evaluation. It is suggestive, not decisive. But it is enough
to say that "use a Hebrew-native model" is not self-evidently the right call for the he-to-en
direction, and that trying the 24B variant and gemma-3-12b-it on Ozen's own utterances is a cheap
experiment that should happen before any prompt is tuned further.

---

## 5. The pipeline that gets closer

### 5.0 Pass 0: span preparation (deterministic, no model)

Everything in [`code-switching.md` section 4](./code-switching.md#4-a-better-algorithm-label-spans-decide-by-grammar-route-per-span):
segment clitics, label spans, normalise unambiguous transliterations to Latin, mark `LAT` spans
protected. This is where the harness class and the `reset -> rust` class die, and it costs no tokens
and no latency.

### 5.1 Pass 1: draft, with register detection

Add one cheap classification before translating: **field** (code / game-design / product / admin) and
**tenor** (order to an agent / note to self / question / message to a person). Both are inferable from
the utterance and both change the correct English. An order becomes imperative and terse; a design
musing keeps its hedges, because the hedges *are* the content.

This is what MAPS does and its result is worth copying precisely: extract preparatory knowledge
(keywords, topic, relevant demonstrations), **then run a quality-estimation step that filters out
unhelpful knowledge before translating** ([arXiv 2305.04118](https://arxiv.org/abs/2305.04118),
verified 2026-08-06). **The filter is the part Ozen is missing.** Its hint block today injects
everything that matches, unfiltered, which is how `לעשות -> system` reached a prompt.

### 5.2 Fix the terminology injection that already exists

`store.rs` and `hint_block` in `translate.rs` are the right architecture. The Dice-plus-margin aligner
is well designed and its tests are genuinely good, including the one pinning that a single repeated
sentence teaches nothing. The problem is not the mechanism, it is **what the mechanism is allowed to
promote**.

Read from `dictionary.json`, 2026-08-06: **52 promoted terms, 3 technical, at least 6 outright wrong**
(`לעשות -> system`, `בעברית -> write`, `שנוכל -> install`, `עושה -> doesn't`, `שזה -> their`,
`לזה -> addition`). One of them measurably corrupted output ("we can **system**... use the instructional
buttons", entry 62) and another produced entry 11.

Four changes, in order of value:

1. **A promotion gate on word class.** Only promote a Hebrew token if it is *not* in the function-word
   list, and if either (a) its English rendering is a known technical term, or (b) it clears
   `phonetics::similarity` against its own rendering, which is the signature of a transliteration
   rather than a translation. `קומיט -> commit` scores high; `לעשות -> system` scores near zero. This
   one gate removes 49 of the 52 current entries and keeps all 3 that are useful.
2. **Align phrases, not just words.** `בנוסף לזה` occurs 7 times and produced two independent
   single-word hints. Bigram alignment on the top-N frequent bigrams would fix this class.
3. **Cap the block hard and log the cap.** `MAX_TERM_HINTS` is 24. Entry 65 ran with 8 hints, entry 64
   with 7. The prompt-crowding failure appears well below 24.
4. **Control the experiment.** WMT23's terminology shared task found that terminology dictionaries
   improved chrF by 0 to 10 points, **but that injecting an equal amount of information from the
   reference gave similar results** ([WMT23 findings](https://aclanthology.org/2023.wmt-1.54/),
   verified 2026-08-06). The measured gain may be "more context helps", not "the right term helps". So
   when the dictionary is measured, it must be measured **against a control that injects an equal
   quantity of unrelated context** - otherwise the wrong thing is being measured.

### 5.3 Pass 2: revise, with correctly-set expectations

A second pass helps, and the literature is very specific about *what* it helps.

> *"Refinement projects outputs toward the refiner's distribution rather than performing targeted error
> repair."* Gains concentrate in **fluency, style and terminology**, with limited and inconsistent
> improvement in **adequacy**. Document-level translation followed by **segment-level** refinement gives
> the strongest and most stable improvements, and **a simple general refinement prompt beats
> error-specific prompting and evaluate-then-refine**.
> - [arXiv 2605.13368](https://arxiv.org/abs/2605.13368), verified 2026-08-06

Read that carefully before building it. A revise pass buys **register and terminology**, which is
exactly what "UN-grade" means and exactly what Ozen is missing. It does **not** buy accuracy. Anything
that must be accurate has to be pinned in pass 0 or pass 1, by protected spans and locked terminology,
not hoped for in pass 2.

Two design consequences, both counter-intuitive and both sourced:
- **Keep the revise prompt simple and general.** Error-specific prompting measured worse.
- **Do not iterate more than once.** Repeated self-correction makes string metrics drop while neural
  metrics and human raters improve ([arXiv 2306.03856](https://arxiv.org/abs/2306.03856), verified
  2026-08-06), and there is no evidence of continued gains from further rounds.

**Latency budget.** Median `llm_ms` in the log is 2,611 ms and median `asr_ms` is 514 ms. A second pass
roughly doubles the LLM leg to about 5 seconds before paste. That is a real cost against the UN's own
"keep up with the speed" criterion and it is Orel's call, not a technical conclusion. A sensible
compromise: run pass 2 **only when the pass-0 oracle flags something** (section 6), which in this
corpus would be a minority of utterances.

---

## 6. The oracle that can say NO

Before any metric, before any model change: five deterministic checks that cost nothing and can
**fail**. `STRIATUM_REWARD.md` calls this building the oracle that says NO, and each of these was
derived from an actual observed failure rather than imagined.

| Check | Would have caught | Cost |
|---|---|---|
| Output of a `translate` contains **zero Hebrew characters** | **Entry 11** (Hebrew returned nearly untouched) | one scan |
| Every protected `LAT` span from the input appears **byte-identical** in the output | `reset -> rust`, `concent -> concentration` | one set comparison |
| No output token is absent from an English wordlist **and** absent from the learned vocab | `Requestrior`, `asin`, `Persistah`, `fiximol`, `arnes` | one lookup per token |
| Output/input content-word ratio within a band learned from the log | `סיקוונצ'לי` deleted silently | two counts |
| Output does not begin with punctuation | already handled by `trim_leading_noise` | already shipped |

**These are not tests, they are runtime guards**, and they must be proven in both directions before
being trusted (SCAR-004): feed each one the failing utterance from column two and **see it fire**, then
feed it a good utterance and **see it stay quiet**. A guard that has never been seen to fire is
indistinguishable from one that cannot.

On a failure the honest behaviours are, in order: retry once with hints disabled; if it fails again,
paste anyway and surface a warning in the orb. Never silently swallow it.

---

## 7. How to evaluate this, and why not BLEU

### Why not BLEU

- BLEU has **R^2 = 0.002 with human fluency judgements** in the 2005 NIST evaluation (0.742 with one
  outlier excluded), and the system ranked **first by humans was ranked sixth by BLEU**. For a single
  example there were at least **40,320** reorderings with an identical BLEU score
  (Callison-Burch, Osborne, Koehn, EACL 2006,
  [aclanthology.org/E06-1032](https://aclanthology.org/E06-1032.pdf), verified 2026-08-06). Their
  thesis verbatim: *"an improved Bleu score is neither necessary nor sufficient for achieving an actual
  improvement in translation quality"*.
- Pairwise system-ranking accuracy against human judgement, n=1717: **COMET 96.5, chrF 89.5, BLEU 88.2**.
  On the **non-Latin-script target** subset, which is the one that matters for Hebrew: **COMET 96.2,
  chrF 95.4, BLEU 92.4** - the neural metric did not degrade on non-Latin scripts. The authors'
  recommendation is verbatim: *"Do not use BLEU, it is inferior to other metrics, and it has been
  overused"* (Kocmi et al., "To Ship or Not to Ship", WMT 2021,
  [aclanthology.org/2021.wmt-1.57](https://aclanthology.org/2021.wmt-1.57.pdf), verified 2026-08-06).
- **The decisive reason for Ozen specifically:** iterative refinement makes BLEU and chrF **drop** while
  neural metrics and human raters say quality **improved**
  ([arXiv 2306.03856](https://arxiv.org/abs/2306.03856), verified 2026-08-06). If the two-pass design in
  5.3 is measured with BLEU, the measurement will say it made things worse. That is not a subtle
  methodological preference; it is the difference between shipping the improvement and reverting it.

### What to use instead

**Primary: COMET (`Unbabel/wmt22-comet-da`), with a paired bootstrap significance test.**
Secondary: chrF, reported but never decisive. Statistical testing raised ranking accuracy by roughly 10
points for every metric in Kocmi et al.'s measurement, so **no delta gets reported without one**.

**A caveat that must not be skipped.** COMET is built on XLM-R, and XLM-R's CC-100 training data
includes Hebrew at 6.1 GB ([data.statmt.org/cc-100](https://data.statmt.org/cc-100/), verified
2026-08-06), with COMET listed as covering 100 languages. But **whether Hebrew appeared in COMET's
human-judgement training data, and what its segment-level correlation on he-en actually is, could not
be verified.** Encoder coverage is not calibration. Treat COMET scores on he-en as **ordinally useful
and absolutely uncalibrated**: use them to compare two Ozen configurations, never to claim an absolute
quality level.

**Do not use an LLM judge as the primary metric.** WMT25 found that while large LLMs are strong at the
**system** level, **reference-based baseline metrics outperform LLMs at the segment level**
([aclanthology.org/2025.wmt-1.24](https://aclanthology.org/2025.wmt-1.24/), verified 2026-08-06). Ozen
scores per utterance, which is segment level. GEMBA remains useful as a **secondary, explanatory**
signal because it produces error spans, but its authors themselves caution against using it to
demonstrate improvements because it depends on a proprietary black-box model
([arXiv 2310.13988](https://arxiv.org/abs/2310.13988), verified 2026-08-06).

### The held-out set, and the action that creates it

The corpus already exists: **85 real utterances with Hebrew input, English output, whisper confidence,
mode, and latency, in `log.json`.** What is missing is references.

**The correction UI in the יומן tab has never been used. `corrected` is `null` on all 85 entries,
`mishearings` is empty, `exemplars` is empty, and `auto_fixed` totals 0 across the whole corpus.** The
entire supervised half of Ozen's learning system is dead, and it is dead for one reason: nobody has
corrected anything.

**One action fixes both problems at once.** Correcting 50 utterances in the יומן tab:
- produces a **50-sentence gold reference set** for he-to-en spoken dev Hebrew, which does not exist
  anywhere else (no Hebrew-English parallel or code-switching corpus is listed in
  [NNLP-IL/Hebrew-Resources](https://github.com/NNLP-IL/Hebrew-Resources), verified 2026-08-06);
- **lights up the mishearing table, the exemplar retrieval, and the locked-term path**, all of which
  are already built, tested, and idle.

That is the highest-value hour available on this project, and it needs no code.

**Stratify the 50** so the set is not all easy sentences: 15 pure-Hebrew, 15 mixed-script, 10 containing
a known transliteration failure, 5 long (over 200 characters), 5 low-confidence (below 0.85).

### The human rubric

Automatic metrics rank; humans decide. The rating axes are taken from the UN exam rather than invented,
grouped per the interpreting-studies convention:

| Category | Axis | Scale |
|---|---|---|
| **Information fidelity** | Nothing added, nothing dropped, nothing inverted | 1-5 |
| | Technical terms correct and **consistent across the session** | 1-5 |
| **Target-language quality** | Grammatical, complete sentences | 1-5 |
| | Register matches field and tenor | 1-5 |
| **Delivery** | Latency acceptable | pass/fail |
| **The operator's own bar** | **Would he have edited this before sending it?** | yes/no |

That last row is the real metric. The brief says *"written in such a way that I don't have to go back
through the text and fix it manually."* **Manual-edit rate is the target variable**, everything else is
a proxy, and it is the one number worth putting on the dashboard.

---

## 8. Ranked, with the reasoning

| # | Change | Why it ranks here |
|---|---|---|
| 1 | The five deterministic oracle checks (section 6) | Catches the worst observed failure (entry 11), costs no tokens, no latency, no model change, and can be proven to fire |
| 2 | Promotion gate on the aligner + protected `LAT` spans | Removes 49 of 52 bad glossary entries and stops the model editing already-correct text. Two small, surgical changes to existing code |
| 3 | 50 corrections in the יומן tab | Creates the eval set and revives three dead subsystems. No code at all |
| 4 | `TRANSLIT` span labelling from `lexicon.json` | Fixes the harness class, the largest single failure family |
| 5 | Register detection + filtered hints (pass 1) | Real gain, real prompt-engineering cost, needs the eval set from row 3 to be verifiable |
| 6 | Two-pass revise, gated on an oracle failure | Real gain in exactly the axis the brief asks about, but it doubles latency and cannot be evaluated without rows 3 and 1 |
| 7 | Try the 24B DictaLM and gemma-3-12b-it | Cheap to run, but the evidence is vendor-run and in the wrong direction. Do it after there is a way to score the result |

Rows 1 through 3 are the ones to do first, and none of them requires touching a prompt.

---

## Related

- [`lexicon.md`](./lexicon.md) / [`lexicon.json`](./lexicon.json) - the 304-entry lexicon
- [`code-switching.md`](./code-switching.md) - the span algorithm this pipeline depends on
- [`product.md`](./product.md) - whether any of this is defensible as a product
