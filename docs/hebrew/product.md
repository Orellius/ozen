# "Offer It to Everyone": The Evidence Report

Written 2026-08-06 (Asia/Jerusalem). This is an evidence report, not a pitch. Every claim carries its
source and the date it was verified. Where something could not be verified it says so.

**Verdict up front, then the evidence:** there is a real, currently uncontested niche - Hebrew-English
code-switched **developer** dictation, on-device - but it is narrow, the entire technical stack is
borrowed from other people's open models, and the one asset that could actually be defensible is
currently switched off. As a personal tool Ozen is already better than anything shipping. As a product
for "everyone", the evidence does not support it yet, and the specific thing that would change that is
named in section 7.

---

## 0. Sample discipline: what was actually sampled

Before any generalisation, what this report rests on:

- **N = 1 speaker.** Orel. 85 utterances in `log.json`, read 2026-08-06, spanning roughly one working
  week, one machine, and two domains (software and game design). One accent, one vocabulary, one
  register.
- **Nothing here is a measurement of "Israeli developers".** It is a measurement of one Israeli
  developer. Every statement about how Hebrew dictation behaves in general is drawn from the published
  literature in sections 2 through 5, not from this corpus.
- **No competitor was tested on Hebrew by me.** Every competitor claim below is either the vendor's own
  published statement or a third-party peer-reviewed measurement, attributed as such. I ran no
  head-to-head.
- **The one thing this corpus does establish**, because it is a direct observation and not an
  inference: **59% of one heavy user's utterances mix Hebrew and English scripts.** That is the premise
  the whole product question rests on, and it is the only premise here with direct evidence behind it.

---

## 1. The incumbents, dated

### Apple - the one that matters most, and the one most likely to be overlooked

**macOS Tahoe lists "Hebrew (Israel)" under Dictation, and again under "Dictation: On-Device and
Modeless Dictation"** ([apple.com/macos/feature-availability](https://www.apple.com/macos/feature-availability/),
verified 2026-08-06).

Read that plainly: **Apple already ships free, on-device Hebrew dictation on the exact same Mac Ozen
runs on.** Any pitch premised on "there is no good Hebrew dictation on the Mac" is dead on arrival.

What Apple does **not** do, per the same page:
- Hebrew does **not** appear in the Translate section.
- Hebrew is **not** listed among Siri's supported languages.

So Apple gives you Hebrew text from Hebrew speech, and nothing else. It does not translate, it does not
polish, it does not learn your terminology, and it has no notion of a technical register. That gap is
real, and it is the honest starting point for any wedge argument.

*Unverified:* reports that iOS 27 adds Hebrew to Apple Translate come from secondary tech press
(9to5Mac, Cult of Mac), not from an Apple page. If that ships, it narrows the gap further, and it is
worth re-checking before any product decision.

### DeepL - yes, it does Hebrew

This one is load-bearing and the intuitive answer is wrong. **DeepL supports Hebrew.** The API
supported-languages table carries `HE / Hebrew` with `translation: true`, `glossaries: true`,
`translationMemory: true` ([developers.deepl.com](https://developers.deepl.com/docs/getting-started/supported-languages),
verified 2026-08-06). DeepL's own launch post states: *"Vietnamese is one of two languages, along with
Hebrew, that we've launched for all versions of DeepL Translator"* and *"Hebrew is the second RTL
language to go live on DeepL, following our successful launch of Arabic"*
([deepl.com blog](https://www.deepl.com/en/blog/vietnamese-thai-hebrew-launch), verified 2026-08-06).

*Unverified:* the launch date (search snippets say mid-2025; no dated primary page confirmed it), and a
third-party claim that Hebrew is restricted to Pro tiers, which DeepL's own "all versions" wording
contradicts. `support.deepl.com` returns HTTP 403 to crawlers.

DeepL takes text, not speech, and has no on-device story. But "no serious MT engine does Hebrew" is not
an available argument.

### Google

- Cloud Translation supports Hebrew under both `he` and `iw`, on the Translation LLM, NMT, and custom
  models ([docs.cloud.google.com/translate](https://docs.cloud.google.com/translate/docs/languages),
  verified 2026-08-06).
- **Google Speech-to-Text is the worst performer on Hebrew of every system independently measured.**
  Peer-reviewed WER (Marmor et al., Interspeech 2025, Table 2,
  [isca-archive.org](https://www.isca-archive.org/interspeech_2025/marmor25_interspeech.pdf), verified
  2026-08-06): ivrit-ai eval-d1 **21.2**, SASpeech 18.9, FLEURS **38.5**, Common Voice 38, KAN 29.2.
- *Unverified:* any published evaluation of Google Translate on Israeli slang or Hebrew-English
  code-switching. No such study was found.

### OpenAI Whisper - the numbers, from the paper

From the Whisper paper's own appendix ([arXiv 2212.04356](https://arxiv.org/pdf/2212.04356), verified
2026-08-06):

| Metric | Hebrew | German | English |
|---|---|---|---|
| ASR WER on FLEURS (large-v2) | **27.1** | 4.5 | 4.2 |
| Speech translation into English, BLEU (large-v2) | **21.8** | 34.6 | - |

Hebrew is roughly **six times worse** than the major European languages on recognition, and materially
behind on direct speech translation. Training-data volume explains it: Hebrew is approximately
**0.1% of Whisper's multilingual corpus** (read off a log-scale figure, so treat the exact digits as
approximate). Third-party measurement of large-v3 on Hebrew FLEURS is **26.2** (Marmor et al.).

**This is the single most important structural fact in the report.** Hebrew is not badly served because
nobody tried; it is badly served because it is a small language with proportionally tiny representation
in the corpora everything else is built on. That is a durable condition, not a temporary one.

### ivrit-ai - the strongest open Hebrew ASR, and Ozen already runs it

An Israeli nonprofit whose stated goal is *"quality Hebrew support in AI tools, mainly in
transcription"*, with a corpus described as *"the largest Hebrew corpus for commercial AI use, over
22,000 hours"* ([ivrit.ai](https://www.ivrit.ai/), verified 2026-08-06); 21 models and 28 datasets
published ([huggingface.co/ivrit-ai](https://huggingface.co/ivrit-ai), verified 2026-08-06).

Peer-reviewed WER (same Interspeech 2025 table): eval-d1 **6.2**, SASpeech **8**, FLEURS 24.1, Common
Voice 20.7, KAN 11.3. **It beats every OpenAI Whisper variant on every Hebrew dataset.** The paper is
also honest that **AWS Transcribe Batch beat it on 3 of 5 datasets**.

**Ozen runs `ivrit-ai/whisper-large-v3-turbo-ggml`.** So Ozen's ASR advantage over a Whisper-based
competitor is real, and it is **entirely borrowed**. It is Apache-2.0 and available to anyone who reads
a model card. It is not a moat; it is a good decision that any competitor can copy in an afternoon.

**And it carries a trap directly relevant to Ozen's architecture.** The turbo model card states
verbatim: *"Language detection capability of this model has been degraded during training - it is
intended for mostly-hebrew audio transcription. Language token should be explicitly set to Hebrew."*
([model card](https://huggingface.co/ivrit-ai/whisper-large-v3-turbo), verified 2026-08-06). Ozen
defaults to `speech_lang: "auto"` and reads `full_lang_id_from_state` from this model. The corpus
confirms the consequence: `lang` is `"he"` on all 85 entries. See
[`code-switching.md` section 3.5](./code-switching.md#35-the-whisper-language-tag-is-not-a-usable-fallback).

### DICTA / DictaLM - and the finding that cuts against the premise

DictaLM 3.0 comes in 24B, 12B and 1.7B, trained on ~100B Hebrew tokens plus 30B English on 80 H200 GPUs
([technical report](https://dicta.org.il/publications/DictaLM_3_0___Techincal_Report.pdf), verified
2026-08-06). The 12B Instruct that Ozen runs was released **2025-12-10**
([model card](https://huggingface.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct), verified 2026-08-06).

**Table 8 of their own report, Translation benchmark** (win-rate against Gemini 2.5 Pro, judged by
GPT-4o):

| Model | Translation score |
|---|---|
| DictaLM-3.0-24B-Thinking | 30.09 |
| gemma3-27b-it | 26.73 |
| **gemma-3-12b-it** | **16.50** |
| **DictaLM-3.0-Nemotron-12B-Instruct** (Ozen's model) | **13.50** |

**The Hebrew specialist loses to a general model at its own weight class, on translation, on its own
benchmark.** Two caveats stated honestly: the direction is English into Hebrew, the opposite of Ozen's;
and it is vendor-run. It is suggestive, not decisive. But "you need a Hebrew-native model to translate
Hebrew well" is not supported by the only numbers that exist.

The same pattern repeats elsewhere: HEBATRON, a 30B Hebrew MoE from PwC Next, scores 73.8 Hebrew
average against DictaLM-3.0-24B-Thinking's 68.9 but **loses to Gemma-3-27B-IT (76.3), which wins 61% of
decisive human-arena votes** ([arXiv 2605.11255](https://arxiv.org/abs/2605.11255), verified 2026-08-06,
dating slightly ambiguous).

Where the Hebrew specialists genuinely and unambiguously win is **ASR** (ivrit-ai beats every Whisper
variant) and **Hebrew-specific tasks** like Nikud (DictaLM 76.12 vs gemma's 51.78). That is a sharper
and more defensible story than "Hebrew needs Hebrew models".

The one neutral venue, the Hebrew LLM Leaderboard (Mafat / Israeli National NLP Program / DICTA), is
**archived and no longer maintained**, and never included GPT, Claude or Gemini.

### The dictation products

| Product | Hebrew | Code-switching | On-device | Price |
|---|---|---|---|---|
| **Apple Dictation** | Yes, on-device | Not addressed | **Yes** | Free |
| **Soniox** | Yes, system-wide voice typing | **Explicitly claimed** | No, cloud | $0.10/hr async, $0.12/hr streaming |
| **Wispr Flow** | Not listed | **Explicitly disclaimed** | No | Not checked |
| **Verbit** (Israeli) | Yes, hybrid ASR + humans | Not addressed | No | Not disclosed |
| **Nuance Dragon** | **No** | - | - | - |
| **AWS Transcribe** | Yes, measured best on 3 of 5 Hebrew sets | Not addressed | No | Not checked |

Two of these rows deserve quoting directly.

**Soniox is the closest thing to a direct competitor.** It claims *"mixed-language conversations where
speakers switch languages mid-sentence"* and *"automatically detects and transcribes language switching
mid-sentence... No configuration or manual language hints are required"*, from a single unified model.
Its marketing even uses a Hebrew-dev-shaped example: `אני צריך להזמין קפה לפני ה-meeting`
([soniox.com/soniox-app/hebrew](https://soniox.com/soniox-app/hebrew), verified 2026-08-06). **The claim
is made; the claim is unverified.** No third-party measurement of it exists. It is cloud-processed, and
it does not translate or learn per-speaker terminology.

**Wispr Flow, the leading developer dictation product, explicitly disclaims exactly this capability:**
*"Flow works best when you speak primarily in one language with occasional words from another, rather
than alternating sentence by sentence"* and *"Rapid language switching within a single sentence is not
supported."* Hebrew is not named anywhere on their language page
([docs.wisprflow.ai](https://docs.wisprflow.ai/articles/3191899797-use-flow-with-multiple-languages),
verified 2026-08-06).

*Unverified:* Soniox's Hebrew accuracy (a "7.5% WER" figure circulates on a vendor-adjacent blog;
Soniox's own Hebrew page publishes no number), Verbit's Hebrew accuracy and pricing, and Superwhisper's
Hebrew support.

---

## 2. Where each incumbent is actually weak on spoken Israeli Hebrew plus dev code-switching

| Incumbent | Weakness, and the evidence for it |
|---|---|
| Apple Dictation | Transcribes Hebrew and stops there. No translation, no register control, no terminology memory. Hebrew absent from Translate and Siri per Apple's own feature page |
| Google STT | Measured worst of all systems on Hebrew: FLEURS WER 38.5 (Marmor et al. 2025) |
| Whisper (vanilla) | Hebrew FLEURS WER 27.1 vs 4.2 English; Hebrew is ~0.1% of the training corpus |
| DeepL | Text only, no speech, no on-device path. Glossaries exist but the docs are silent on inflection handling, which matters enormously for Hebrew clitics |
| ivrit-ai | ASR only, and its own model card says language detection was **deliberately degraded** - which is precisely the wrong property for code-switched input |
| DictaLM | By DICTA's own Table 8, loses at translation to a same-size general model |
| Soniox | Cloud only. Claims code-switching with zero third-party verification. No translation, no per-speaker terminology |
| Wispr Flow | Openly does not do mid-sentence switching, and does not list Hebrew |
| Verbit | Aimed at media and legal transcription, not at live dictation into a focused app |

**The one shared blind spot:** every one of them stops at transcription or at translation. **None of
them learns the individual speaker's terminology across sessions**, which is the thing that would have
fixed הרנס after its first correction rather than producing four renderings in one session.

---

## 3. What is genuinely uncontested

**Nobody has independently measured Hebrew-English code-switched dictation.** That is not rhetoric, it
is a specific gap that was searched for and not found:

- No Hebrew-English code-switching corpus exists in LinCE
  ([arXiv 2005.04322](https://arxiv.org/abs/2005.04322)), in the CALCS shared tasks, or in the Israeli
  national resource index ([NNLP-IL/Hebrew-Resources](https://github.com/NNLP-IL/Hebrew-Resources)),
  all verified 2026-08-06.
- No Hebrew slang corpus and no Hebrew-English parallel corpus is listed there either.
- Soniox claims the capability with no published number; Wispr Flow disclaims it; no benchmark exists to
  settle it.

So the honest framing is not "we are better at this". It is: **there is no way for anyone, including
Ozen, to currently claim they are better at this.** That is an opportunity and a hazard at once, because
an unmeasurable advantage is also an unsellable one.

---

## 4. Is there a wedge?

Test each candidate against the evidence above rather than against enthusiasm.

| Candidate wedge | Verdict |
|---|---|
| "Hebrew dictation on the Mac" | **Dead.** Apple ships it free, on-device, on the same machine |
| "Better Hebrew ASR" | **Not ours.** ivrit-ai owns it, Apache-2.0, and Ozen is a consumer of it |
| "Hebrew to English translation" | **Weak.** DeepL does Hebrew; a general 12B model beats the Hebrew specialist on DICTA's own benchmark |
| "On-device / privacy" | **Real but thin.** Apple is also on-device. It is a requirement, not a differentiator |
| "Dictation for developers" | **Occupied.** Wispr Flow owns this, in English |
| **"Code-switched Hebrew-English dictation for developers, with per-speaker terminology memory, on-device, pasting into the focused app"** | **The only candidate that survives.** Every clause is doing work: drop code-switching and Apple wins; drop the terminology memory and Soniox wins; drop on-device and it is a worse Soniox; drop developers and there is no code-switching to speak of |

### The thing that could actually be defensible, and why it is not yet

The models are all borrowed. The one asset that compounds and cannot be copied from a model card is
**the self-correcting per-speaker terminology memory**: correct a word once, it is repaired forever;
speak a term often, it becomes a locked rendering. That is a genuine architectural bet, it is already
built and tested, and no competitor in section 1 has it.

**It is currently inert.** Measured from `dictionary.json` and `log.json` on 2026-08-06:

- **0 corrections across all 85 utterances.** `corrected` is null everywhere.
- **`mishearings: 0`, `exemplars: 0`, `auto_fixed` totals 0.** The supervised half has never run.
- Of the 52 terms the unsupervised aligner did promote, **3 are technical and at least 6 are outright
  wrong**, and one of the wrong ones demonstrably corrupted output (see
  [`interpretation-quality.md` section 3](./interpretation-quality.md#3-entry-11-the-single-most-important-data-point-in-this-corpus)).

So the differentiating asset has produced, so far, net negative value. **That is the finding, and it is
not a reason to abandon the bet.** It is a diagnosis: the supervised path is untested because it has
never been used, and the unsupervised path needs a promotion gate. Both are named and both are small.
But until it works on N=1, "it learns your vocabulary" is a claim with no evidence behind it and should
not appear in any product description.

---

## 5. Market size, honestly bounded

- Israeli high-tech employed about **403,000 people in H1 2025**, roughly 11.5% of the national
  workforce, of whom about **280,000 are in high-tech services (software)**
  ([Israel Innovation Authority, State of High-Tech 2025](https://innovationisrael.org.il/en/report/part-1-high-tech-employment/),
  verified 2026-08-06). Employment has been flat since 2022 and fell in 2024.
- That is the **outer** bound. The addressable set is the intersection of: speaks Hebrew natively,
  writes code, **dictates instead of typing**, and works on a Mac.
- **I have no measurement of the dictation-adoption rate among developers, in Israel or anywhere.** No
  such figure was found and none is invented here. Without it, every number past the outer bound would
  be fiction.

What can be said without a number: dictation is a minority input method among developers today, the
Hebrew-speaking developer population is at most low hundreds of thousands and is flat, and the
intersection is small enough that this is a **niche tool business, not a platform business**. Anyone
arguing otherwise needs the adoption number that does not currently exist.

---

## 6. What would have to be true

For "offer it to everyone" to be a defensible plan rather than an aspiration, all four of these:

1. **The terminology memory works, demonstrably, on N=1.** Corrections stick, the glossary holds
   technical terms rather than function words, and הרנס comes out as "harness" every time after one
   correction. **Currently false.**
2. **Code-switching accuracy is measured**, on a hand-labelled set, with a number that can be quoted.
   No benchmark exists, so it has to be built. **Currently false**, and section 3 explains why nobody
   else can quote a number either.
3. **A second Hebrew-speaking developer uses it and it works for them.** Every failure in this corpus
   is one accent, one vocabulary, one domain. The seed vocabulary is dev-only while half the corpus is
   game design, which is already evidence that the tuning is speaker-specific. **Currently untested.**
4. **The distribution problem has an answer.** Public macOS distribution needs a Developer ID and
   notarization at $99/yr, since macOS 15 removed the right-click-open bypass. Ozen is ad-hoc signed
   with a self-signed `Whissper Local` identity, which is documented as acceptable **only while the
   repo is private**. **Currently a known blocker**, and a cheap one.

Item 3 is the one that decides it. Everything else is engineering; that one is the actual product
question, and the cheapest way to answer it is to hand a build to one other Hebrew-speaking developer
and watch.

---

## 7. Verdict

**As a personal tool:** Ozen is already doing something no shipping product does, and the two research
documents beside this one describe roughly a week of work that would make it substantially better. That
work is worth doing regardless of any product decision, because the tool is used daily and the failures
are measured and specific.

**As a product for everyone:** the evidence does not support it today. The stack is borrowed, the
differentiator is inert, the market is small and flat, and the central capability cannot be measured
because no benchmark exists. That is four independent reasons, and none of them is fatal on its own.

**The single highest-value next step is not a product decision at all.** It is to correct 50 utterances
in the יומן tab. That one hour simultaneously creates the evaluation set that does not exist anywhere
in the world, revives three dead subsystems, and produces the first real evidence for or against the
one bet that could make this defensible. **Everything in section 6 is downstream of it, and it needs no
code.**

If that works, item 3 - one other Hebrew-speaking developer - is the next question, and it costs a
build and a conversation.

---

## Explicitly unverified in this report

Stated so no reader mistakes a gap for a finding: DeepL's Hebrew launch date and any tier restrictions;
Soniox's Hebrew accuracy and its code-switching claim; Verbit's Hebrew accuracy and pricing;
Superwhisper's and Wispr Flow's Hebrew support; whether iOS 27 adds Hebrew to Apple Translate; any
independent GPT/Claude/Gemini versus DictaLM Hebrew comparison (none appears to exist); Hebrew-to-English
translation quality for any model other than Whisper's own BLEU 21.8; and the dictation-adoption rate
among developers, which is the number section 5 most needs and could not find.

---

## Related

- [`lexicon.md`](./lexicon.md) / [`lexicon.json`](./lexicon.json) - the 304-entry lexicon
- [`code-switching.md`](./code-switching.md) - Hebrew versus English inside one sentence
- [`interpretation-quality.md`](./interpretation-quality.md) - the rubric, the pipeline, the metric
