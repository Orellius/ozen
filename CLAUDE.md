# CLAUDE.md - orellius-stt

Hebrew speech -> coherent English, pasted into the focused app. Menu-bar Tauri 2 tool,
fully on-device. See `README.md` for the full pipeline; this is the agent contract.

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
  `clean_transcript` in `lib.rs` drops them; the RMS floor catches the rest.

## Models (must be present)

- Ollama: `hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K` (translator).
- HF cache: `ivrit-ai/whisper-large-v3-turbo-ggml` (ASR). Upgrade path: the newer `20250513`
  fine-tune ships as CT2/faster-whisper; converting it to GGML is the next model bump.

## Verify

`./scripts/build-run.sh`, then hold Right-⌘, speak Hebrew into a focused text field,
release, confirm English pastes. Translation alone: POST Hebrew to Ollama `/api/chat`.
