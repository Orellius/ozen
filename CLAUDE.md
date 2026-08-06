# CLAUDE.md - orellius-stt (repo: `Orellius/whissper`, private)

Hebrew speech -> polished Hebrew or coherent English, pasted into the focused app.
Menu-bar Tauri 2 tool, fully on-device. See `README.md` for the full pipeline; this is
the agent contract. Repo is `whissper` because `Orellius/orellius-stt` holds the old
pre-Tauri iteration (2026-06-05) - do not push there.

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

## Non-obvious constraints (v0.3.0 additions first)

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
- `ORELLIUS_STT_PROMPT` biases decoding toward Hebrew dev-speak with Latin tech terms
  (default in `lib.rs`); empty string disables.
- Hallucination gate: per-segment `no_speech_probability() > 0.5` drops the segment
  (`whisper.rs`); the `clean_transcript` blocklist in `lib.rs` stays as backstop.
- Translate OFF now pastes POLISHED Hebrew via DictaLM (`polish_hebrew` in `translate.rs`:
  punctuation, ASR fixes, transliterated tech terms restored to Latin script). Same
  never-execute hardening as translation - do not loosen either prompt.
  `ORELLIUS_STT_POLISH=0` restores raw-Hebrew passthrough.

## Verify

`./scripts/build-run.sh`, then hold Right-⌘, speak Hebrew into a focused text field,
release, confirm English pastes. Translation alone: POST Hebrew to Ollama `/api/chat`.
