# CLAUDE.md - orellius-stt (repo: `Orellius/whissper`, private)

Hebrew speech -> polished Hebrew or coherent English, pasted into the focused app.
Menu-bar Tauri 2 tool, fully on-device. See `README.md` for the full pipeline; this is
the agent contract. Repo is `whissper` because `Orellius/orellius-stt` holds the old
pre-Tauri iteration (2026-06-05) - do not push there.

## What it is

Push-to-talk (hold Right-⌘) -> native cpal capture -> whisper (ivrit-ai, `he`) ->
clean -> DictaLM 3.0 (local, `he->en`) -> clipboard + Cmd+V into the front app.

Revival of the old Electron "Whissper" (`~/Archive/_OSS/orellius-stt`), rebuilt on the
Tauri whisper core from `~/Archive/_OSS/orellius-voice`, with a NATIVE audio path
(no webview dependency) and a 2026 model stack.

## Files (src-tauri/src)

- `lib.rs` - imperative shell: tray, hotkey wiring, app state, the record->ASR->translate->paste pipeline.
- `audio.rs` - cpal capture on a dedicated thread (Stream is !Send); 16k mono resample.
- `whisper.rs` - whisper-rs + Metal, ivrit GGML, `he`. Loaded once via OnceLock.
- `translate.rs` - DictaLM via Ollama `/api/chat`. Prompt is hardened to TRANSLATE, never EXECUTE.
- `hotkey.rs` - CGEventTap push-to-talk (press+release edges). Needs Accessibility.
- `paste.rs` - arboard clipboard (save/restore) + CGEvent Cmd+V.

## Non-obvious constraints

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
