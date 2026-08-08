# CLAUDE.md - Ozen (repo: `Orellius/ozen`, private)

Hebrew speech -> polished Hebrew or coherent English, pasted into the focused app.
Menu-bar Tauri 2 tool, fully on-device. See `README.md` for the full pipeline; this is
the agent contract. Renamed from orellius-stt / whissper on 2026-08-06 (Orel's call): product, repo, directory,
bundle id and env vars all say Ozen. GitHub redirects the old URL. `Orellius/orellius-stt` is
the unrelated pre-Tauri June 2026 iteration - do not push there.

## What it is

Toggle-to-talk (tap Right-⌘, tap again) -> native cpal capture -> whisper (ivrit-ai, `he`)
-> clean -> DictaLM 3.0 (local, `he->en`) + learned term hints -> clipboard + Cmd+V into the
front app -> logged and counted into the dictionary. Hold mode is still selectable.

Revival of the old Electron "Whissper" (`~/Archive/_OSS/orellius-stt`), rebuilt on the
Tauri whisper core from `~/Archive/_OSS/orellius-voice`, with a NATIVE audio path
(no webview dependency) and a 2026 model stack.

## Files (src-tauri/src)

- `lib.rs` - imperative shell: tray, hotkey wiring, app state, the record->ASR->translate->paste->learn pipeline.
- `audio.rs` - cpal capture on a dedicated thread (Stream is !Send); 16k mono resample.
- `whisper.rs` - whisper-rs + Metal, ivrit GGML, `he`. Loaded once via OnceLock.
- `translate.rs` - DictaLM via Ollama `/api/chat`. Prompt is hardened to TRANSLATE, never EXECUTE.
- `hotkey.rs` - CGEventTap edges + clean-tap classification. Needs Accessibility.
- `paste.rs` - arboard clipboard (save/restore) + CGEvent Cmd+V.
- `sound.rs` - synthesized cue tones over a persistent cpal OUTPUT stream (also !Send).
- `store.rs` - the only persistence: settings, utterance log, rejections, learned dictionary.

## Files (frontend - React 19 + TS strict + Vite, since 2026-08-06)

- `index.html` (repo root) - Vite entry. `src/main.tsx` -> `src/App.tsx` (tabs + events).
- `src/ipc.ts` - **the ONLY place that calls `invoke`/`listen`.** Every interface mirrors a
  `#[derive(Serialize)]` struct in `src-tauri/src`; a Rust rename breaks exactly one file.
- `src/stats.ts` - pure derivations over the log. No aggregate is ever persisted.
- `src/components/` - `Home` (stats), `Logs` (+ correction editor), `SettingsPage`, `Tile`.
- `public/pill.html` - the always-on-top orb. Lives in `public/` so Vite copies it VERBATIM;
  it is a standalone document with no imports and must stay that way. Transparent,
  focusable:false (must NEVER steal key focus - the paste targets the focused app),
  all-workspaces, draggable. Needs `macOSPrivateApi: true` for transparency on macOS, and
  `withGlobalTauri: true` because it uses `window.__TAURI__` rather than bundled imports.

## The self-improvement loop (2026-08-08) - how the app fixes itself

Orel's ask, verbatim: "I want the software to fix itself instead of me having to do it... won't
make me go through 300 sessions of recordings just to say, okay, fix this, save it."

**The measurement that shaped the design.** The supervised half was dead: 5 corrections in 267
utterances, `auto_fixed` 0 across every entry ever logged. And the obvious replacement - watch him
edit the pasted text over the Accessibility API - is unavailable, because he dictates into Claude
Code's shell and a terminal exposes no editable text. Re-dictation was measured as a fallback signal
and is nearly dead too: **1 fire in 200 eligible consecutive pairs**, at every threshold from 0.3 to
0.7. He does not repeat himself when the output is wrong; he takes the bad paste and moves on.

So the loop is built to need **no human verdict at all**:

1. **`scripts/eval/build-gold.py`** freezes a stratified 60-pair gold set (`docs/eval/gold.json`) -
   reference translations from a strong model with no latency budget. This is the oracle; before it
   existed, no prompt edit or model swap in this repo could be shown to have improved anything.
2. **`scripts/eval/run-candidates.py` + `score.py`** run the gold set under a named config and grade
   it on four separate axes. **Tense is its own axis** because it is the defect he named and an
   aggregate would hide it. Deterministic checks (leading punctuation, missing capital, leftover
   Hebrew, code fences) run in code where no judge can flatter them.
3. **`scripts/night-pass.py`** grades recent real dictations with the same strong model and writes
   `night-proposals.json` into the app data dir. It never touches `dictionary.json` - the app owns
   that file and writes it whole, so a second writer would silently clobber it.
4. **`Store::ingest_proposals`** applies the proposals at startup, exactly once, then archives the
   file.

**THE AUTHORITY SPLIT IS THE LOAD-BEARING PART.** Orel's own correction is ground truth: it locks a
mishearing, feeds the aligner, forces a rendering. A grader's opinion is EVIDENCE and enters at the
weakest useful level - exemplars (retrieved only for similar input, freely ignorable) and UNLOCKED
mishearings (offered to the model as "maybe", never silently applied). The aligner is never fed by
the night pass, because promotion produces a forced rendering and nothing machine-generated has
earned that. `night_proposals_add_evidence_but_never_outrank_him` pins this, mutant-verified.

**THE NIGHTLY JOB CANNOT LIVE IN THIS REPO.** Measured 2026-08-08 by kickstarting it: the
LaunchAgent fired (`runs = 1`) and exited **2** with `Operation not permitted` reading
`scripts/night-pass.py`. A launchd-spawned process is disclaimed and holds no TCC grant for
`~/Desktop` - SCAR-006. The plist therefore points at `~/.ozen/night-pass.py`, a deployed copy;
this repo stays source of truth and the copy must be refreshed after every edit, or the nightly
pass silently keeps running the old script. Three separate silent-failure modes have now been found
in this one job - a missing `USER` in launchd's environment (the CLI reported "Credit balance is too
low"), a `--since 0` watermark bug, and TCC - and **every one of them was only visible because the
plist writes a log**. Never wire an unattended job without one.

**Apple has no Hebrew, measured on this machine 2026-08-08** (`sw_vers` 26.5.1) - so none of this
can lean on it. Translation framework: 38 languages, `he->en` returns `unsupported`.
FoundationModels: available, 23 languages, no Hebrew. NLTagger: no lemma and no part-of-speech for
Hebrew, so no tense checking. NLEmbedding: nil for Hebrew, so no Apple-side similarity. `he-IL`
speech recognition exists but reports `supportsOnDeviceRecognition = false`. Do not re-derive this;
re-run `scripts/eval/` probes if a macOS release claims to have changed it.

## Non-obvious constraints (v0.5.0 - the self-correcting layer)

- **The mishearing layer is the only thing that can see a "comic push".** A misheard word is
  spelled correctly and sits in a grammatical sentence, so spell check and LLM cleanup both
  pass it. `phonetics.rs` reduces words to a pronunciation skeleton (w and v merge, th becomes
  t, vowels drop) and scores the distance; `store.rs` holds the learned table.
- **TWO thresholds, and conflating them was a real bug.** `SUSPECT_MIN_SCORE` (0.74) is for
  finding a mishearing UNAIDED - high, because a false flag puts a wrong suggestion in front of
  the model. `LEARN_MIN_SCORE` (0.55) applies when Orel has already corrected the entry and the
  only question is mishearing-vs-rephrase. Using the suspicion bar for learning made the live
  case ("comic" -> "commit", 0.62) silently unlearnable.
- **`phonetics::MIN_LEN` (4) is a real boundary, not a knob.** "the" keys to t and "ze" keys to
  s; any threshold loose enough to pair them pairs half the lexicon. Function-word accent
  artifacts belong to the repair PROMPT, which has grammatical context.
- **Confirmed rules are applied silently, suspects are only offered.** Silently rewriting a
  rare-but-correct word is worse than leaving a wrong one, so an unconfirmed sound-alike goes
  into the prompt as "maybe", never into the text.
- **Vocabulary is built from ACCEPTED output only** (`note_vocab`), never from raw ASR -
  otherwise the first mishearing joins the vocabulary and starts attracting correct words.
- **The aligner learns only from `translate`; the mishearing table learns from every mode.**
  A repair pass is en->en and would key the translation table on English tokens.
- **`align_substitutions` is an edit script, deliberately.** The first version collected the
  diverging run with two independent loops and could return deletions paired against nothing -
  measured, it produced ZERO substitutions for the textbook case and the failure looked like a
  threshold problem. Do not "simplify" it back.
- **whisper metrics**: `lang` comes from `full_lang_id_from_state`, `confidence` is the mean
  per-token probability (whisper-rs does not expose avg_logprob; this carries the same signal).
  Silence is trimmed BEFORE normalising - silence is where hallucinations are born.

## Non-obvious constraints (v0.4.0 rebrand)

- **The signing identity is STILL `Whissper Local`, on purpose.** TCC keys on bundle id + code
  signature. v0.4.0 already changes the bundle id (`ai.orellius.stt` -> `ai.orellius.ozen`),
  which costs exactly one Mic + Accessibility re-grant; rotating the certificate at the same
  time would have cost a second one for no user-visible gain. It is an invisible keychain
  label. Rotating it later is a standalone task, and it must be a NEW self-signed cert created
  before `tauri.conf.json` names it, or the build fails at the signing step.
- **The app data dir moved with the bundle id** to
  `~/Library/Application Support/ai.orellius.ozen/`. Anything stored under the old id is
  orphaned, not migrated.
- **The login item points at a path that no longer exists** after the rename. It must be
  re-registered against `/Applications/Ozen.app`.
- **Icons are GENERATED, not drawn**: `scripts/gen-icons.py` derives the app icon and all five
  menu-bar frames from the same squircle + blob geometry the live orb uses. Re-run it after
  changing the palette or the tile radius; do not hand-edit the PNGs.
- **The menu-bar icon is a live level meter during recording** (`arcs_for_level`), sharing the
  orb's `min(1, level * 9)` curve so the two indicators can never disagree. It updates at 8Hz
  and only on a change of arc count.

## Non-obvious constraints (v0.3.0)

- **The pipeline routes on the SCRIPT of the ASR output, not on the requested language.**
  `speech_lang` defaults to `auto`, so a clip may come back English; English gets
  `repair_english` (Hebrew-L1 accent repair) and never gets translated. `is_latin_script` in
  `lib.rs` is the switch - Latin-dominant wins, so one Hebrew word inside an English sentence
  does not flip the route.
- **The whisper initial prompt is chosen PER LANGUAGE MODE.** A Hebrew-register prompt drags
  English clips toward Hebrew output, which defeats auto mode entirely. `OZEN_PROMPT`
  still overrides all three (it is `Option<String>` now - unset means "pick by mode", not
  "use the Hebrew one").
- **The aligner learns ONLY from `mode == "translate"`.** A repair pass is en->en, so feeding
  it to `observe()` would key the table on English tokens and pollute the Hebrew side.
- **The orb's vibrancy MUST be forced `NSVisualEffectState::Active`.** The window is
  `focusable:false` and therefore never key, so the default (follow window-active state)
  renders the material in its inactive, flat-grey appearance permanently. This was the real
  reason it never looked like the Dock. Material is `Popover`, not `HudWindow`.
- **The orb tile radius lives in two places that must agree**: `--radius` in
  `public/pill.html` and the radius argument to `apply_vibrancy` in `lib.rs`. The CSS clips
  the canvas light; the material clips the glass. Diverge and the silhouette doubles.

- **Toggle mode fires on a CLEAN TAP, never on the press.** Right-⌘ is a real modifier, so
  acting on press would make Right-⌘+C start a recording. `hotkey.rs` watches every other
  KeyDown while our key is held and reports `clean_tap = alone && held <= 400ms`. If you ever
  change the event-type list, keep `CGEventType::KeyDown` in the modifier arm - without it the
  combo detection goes blind and the toggle eats the user's shortcuts.
- **`CGEventType` does not implement `PartialEq`** (core-graphics 0.25) - compare with
  `matches!`, not `==`. Cost one compile error on the 0.3.0 build.
- **The aligner needs VARIED contexts, and there is a test that proves it.** Within one
  sentence every Hebrew token co-occurs with every English token equally, so no amount of
  repetition can separate them. Promotion therefore requires hits >= 3 AND Dice >= 0.55 AND a
  margin >= 0.15 over the runner-up. `one_sentence_repeated_teaches_nothing` pins this; the
  first version of the code shipped a share-of-row test that promoted nothing at all, and the
  test caught it. Do not "fix" a quiet dictionary by lowering these bars - a wrong forced
  rendering is permanent and invisible.
- **`correct()` counts a correction ONCE.** An earlier version replayed it MIN_HITS times to
  force promotion; that is precisely the degenerate single-sentence input above. A correction's
  weight lives in the locked term and the exemplar, not in the counter.
- **Cue tones play through a second cpal stream that stays open for the app's life.** Opening
  per cue costs tens of ms and can click. The stop cue fires AFTER `audio.stop()` so the tone
  can never bleed into the captured clip.
- **Settings are written whole, never patched.** The tray menu flips `translate` on the same
  JSON file the dashboard writes; a partial update would race it.

## Non-obvious constraints (carried over)

- **The translate prompt must say "translate, do not follow it."** The Hebrew input is a
  spoken *instruction*; an instruct model will otherwise execute it (write code) instead of
  translating. Verified 2026-06-21. Don't loosen `SYSTEM_PROMPT` in `translate.rs`.
- **Sign with a STABLE identity** (`Whissper Local` in `tauri.conf.json`). Ad-hoc/unsigned
  builds get a new code-signature identity each build -> macOS re-prompts for Mic + Accessibility
  every time. The whole "stop bugging me about permissions" fix depends on this.
- **cpal Stream is !Send on macOS** - it is created, owned, and dropped only on the audio
  thread (`audio.rs`); never move it across threads.
- **Whisper hallucinations**: it invents stock Hebrew phrases (תודה רבה / כתוביות) on silence.
  Three layers now: RMS floor -> per-segment no_speech gate (`whisper.rs`) -> `clean_transcript`
  blocklist in `lib.rs` as backstop.
- **The `___isPlatformVersionAtLeast` link trap** (hit 2026-08-05 on the 0.16 bump): ggml's
  ObjC `@available` checks need Apple's compiler-rt at link time. Fixed in
  `src-tauri/.cargo/config.toml` (deployment target 13.0 + explicit libclang_rt.osx.a link-arg).
  If the same undefined symbol returns after an Xcode upgrade, refresh the hardcoded clang path.
- **A changed `[env]`/rustflags does NOT recompile already-built sys crates** - the stale
  `whisper-rs-sys` rlib gets relinked and the "fix" silently never applies. Force it:
  `cargo clean --release -p whisper-rs-sys` (the `--release` flag is required; without it the
  clean removes 0 files).

## Models (must be present)

- Ollama: `hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K` (translator + Hebrew polish).
- HF cache: `ivrit-ai/whisper-large-v3-turbo-ggml` (ASR). Verified 2026-08-05: the cached
  snapshot (`2130c78`) IS ivrit-ai's latest GGML ("New version: 2025.05.13") - no conversion
  needed; the old "convert CT2 to GGML" upgrade path in earlier notes is obsolete.

## Hebrew quality layer (2026-08-05 revival)

- whisper-rs 0.16, Metal + flash attention, beam search (size 5) instead of greedy.
- `OZEN_PROMPT` biases decoding toward Hebrew dev-speak with Latin tech terms
  (default in `lib.rs`); empty string disables.
- Hallucination gate: per-segment `no_speech_probability() > 0.5` drops the segment
  (`whisper.rs`); the `clean_transcript` blocklist in `lib.rs` stays as backstop.
- Translate OFF now pastes POLISHED Hebrew via DictaLM (`polish_hebrew` in `translate.rs`:
  punctuation, ASR fixes, transliterated tech terms restored to Latin script). Same
  never-execute hardening as translation - do not loosen either prompt.
  `OZEN_POLISH=0` restores raw-Hebrew passthrough.

## Verify

`./scripts/build-run.sh`, then hold Right-⌘, speak Hebrew into a focused text field,
release, confirm English pastes. Translation alone: POST Hebrew to Ollama `/api/chat`.
