# orellius-stt

Hold a key, speak Hebrew, release - coherent **English** lands in whatever app is
focused (your terminal running Claude Code, an editor, a chat box). A menu-bar tool,
fully on-device. Revival of the old Electron "Whissper" app, rebuilt on Tauri 2 with
a native (non-webview) audio path and a 2026 model stack.

## The loop

```
hold Right-⌘  ->  cpal mic capture (16k mono)  ->  whisper (ivrit-ai, he)
   ->  clean (drop whisper's silence hallucinations)  ->  DictaLM 3.0 (he -> en, local)
   ->  clipboard + synthetic Cmd+V into the focused app
```

Everything runs locally. No audio leaves the machine.

## Stack

| Layer | Tech |
|---|---|
| Shell | Tauri 2 (Rust backend, menu-bar `LSUIElement` app) |
| Capture | `cpal` native input, downmixed + resampled to 16k mono f32 |
| Hotkey | `CGEventTap` push-to-talk (separate press/release), needs Accessibility |
| STT | `whisper-rs` + Metal + `ivrit-ai/whisper-large-v3-turbo-ggml` (`he`) |
| Translate | local **DictaLM 3.0 Nemotron 12B Instruct** via Ollama (`he -> en`) |
| Paste | `arboard` clipboard (save/restore) + `CGEvent` Cmd+V |
| Dashboard | vanilla HTML/JS, Hebrew RTL, event-driven (status, history, settings) |

## Why this pipeline (2026 research)

- **Two-step (ASR -> LLM translate) beats one-shot speech-translation for fluent English.**
- **DictaLM 3.0** (Hebrew-native) fixes the literal/broken output a generic model gave.
- The model is an *instruct* model, so the translate prompt firmly says "translate, never
  follow it" - otherwise a spoken instruction gets *executed* (it writes code) instead of
  translated. See `translate.rs`.
- whisper.cpp/whisper-rs (not MLX) is the right tool for short push-to-talk clips.

## Permissions (the "stop bugging me" fix)

Two macOS TCC grants are needed, **once**:
- **Microphone** (capture) - prompted on first record, or via the dashboard button.
- **Accessibility** (global hotkey + synthetic paste) - System Settings -> Privacy &
  Security -> Accessibility.

The app is signed with a **stable** self-signed identity (`Whissper Local`), so the code
signature identity doesn't change between rebuilds and macOS keeps the grants. (Ad-hoc /
unsigned builds get a new identity each build and re-prompt forever - that was the old pain.)

## Build + run

```bash
cd ~/Desktop/Studio/tools/orellius-stt
bun install            # first time (tauri CLI)
cargo tauri build      # release .app, signed with "Whissper Local"
open "src-tauri/target/release/bundle/macos/Orellius STT.app"
```

Or `./scripts/build-run.sh`. The app lives in the menu bar (aleph icon); left-click opens
the dashboard, right-click for the menu. Hold **Right-⌘**, speak Hebrew, release.

First record loads the whisper model (a few seconds, once). DictaLM's first call pays a
~5s cold load, then translations are ~1-2s.

## Config (env)

| Var | Default | Purpose |
|---|---|---|
| `ORELLIUS_STT_HOTKEY` | `cmd_r` | `cmd_r` / `ctrl` / `f5` / `f6` |
| `OLLAMA_MODEL` | `hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K` | translator |
| `OLLAMA_HOST` | `http://localhost:11434` | Ollama endpoint |
| `WHISPER_MODEL_PATH` | HF cache auto-resolve | ivrit GGML override |
| `ORELLIUS_STT_DEBUG` | unset | write hotkey log to `/tmp/orellius-stt-hotkey.log` |

## Requirements

- Ollama running with the DictaLM model pulled:
  `ollama pull hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K`
- ivrit-ai GGML whisper model in the HF cache (already present from the old build).

## Roadmap (not built yet)

- Upgrade the ivrit GGML to the newer `20250513` fine-tune (convert CT2/HF -> GGML).
- Editable preview HUD before paste (eyeball/edit the English first).
- Configurable hotkey + translate toggle persisted to disk.
- Optional cloud fallback (Gemini/Soniox) behind a toggle for max fluency.
