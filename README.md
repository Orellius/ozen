# Ozen (אוזן - "ear") · repo `Orellius/ozen`

Tap a key, speak Hebrew, tap again - polished **Hebrew** or coherent **English** lands in
whatever app is focused (your terminal running Claude Code, an editor, a chat box). A
menu-bar tool, fully on-device. Descendant of the Electron "Whissper" app, rebuilt on Tauri 2
with a native (non-webview) audio path and a 2026 model stack. Renamed from orellius-stt /
whissper on 2026-08-06: the product, the repo and the directory all say Ozen now, and GitHub
redirects the old URLs. `Orellius/orellius-stt` still holds the pre-Tauri June 2026 iteration
and is unrelated.

## The loop

```
tap Right-⌘  ->  cpal mic capture (16k mono)  ->  whisper (ivrit-ai, he|en|auto)
   ->  clean (drop whisper's silence hallucinations)
   ->  route by SCRIPT of the result:
        Hebrew out  ->  DictaLM 3.0 translate (he -> en) or polish (he -> he)
        English out ->  DictaLM 3.0 accent repair (Hebrew-L1 English)
      ...both with learned term hints for this sentence
   ->  clipboard + synthetic Cmd+V into the focused app
   ->  log the pair, count it toward the dictionary
```

Everything runs locally. No audio leaves the machine.

Each stage boundary also plays a short **cue tone**, so the loop can be followed by ear
while you are looking at the terminal rather than at the orb.

## Stack

| Layer | Tech |
|---|---|
| Shell | Tauri 2 (Rust backend, menu-bar `LSUIElement` app) |
| Capture | `cpal` native input, downmixed + resampled to 16k mono f32 |
| Hotkey | `CGEventTap` push-to-talk (separate press/release), needs Accessibility |
| STT | `whisper-rs` + Metal + `ivrit-ai/whisper-large-v3-turbo-ggml` (`he`) |
| Translate | local **DictaLM 3.0 Nemotron 12B Instruct** via Ollama (`he -> en`) |
| Paste | `arboard` clipboard (save/restore) + `CGEvent` Cmd+V |
| Cues | synthesized sine motifs through a persistent `cpal` output stream (`sound.rs`) |
| Orb | Dock-tile squircle, native `NSVisualEffectView` glass + a canvas light (`public/pill.html`) |
| Memory | JSON store: settings, utterance log, learned dictionary (`store.rs`) |
| Dashboard | React 19 + TypeScript strict + Vite, Hebrew RTL, three tabs |

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
bun tauri build        # release .app, signed with "Whissper Local"
# bundle lands under the studio-cache cargo target dir:
open "$HOME/.studio-cache/cargo/release/bundle/macos/Ozen.app"
```

Or `./scripts/build-run.sh` (resolves the real target dir itself). Build trap, documented
in `src-tauri/.cargo/config.toml`: ggml's `@available` checks emit
`___isPlatformVersionAtLeast`, which rustc does not link on its own - the config pins
`MACOSX_DEPLOYMENT_TARGET=13.0` and links Apple's compiler-rt explicitly. Don't delete
either line, and refresh the compiler-rt path on Xcode major bumps.

An **always-on-top orb** floats top-center (draggable): a Dock-tile squircle of real macOS
vibrancy glass with a living light inside it. The light is one asymmetric blob deformed by
harmonics at unrelated frequencies, drifting on a slow Lissajous path and lit from an
off-centre core, so it reads as glowing from within rather than as a coloured shape. It
follows the real mic level while recording (fast attack, slow release, so it keeps glowing
between words) and shifts colour per stage. It never takes focus, so the paste always lands
in your app.

Two non-obvious details make the glass work: the material is `Popover` (the milky menu glass,
not the flat `HudWindow` HUD material), and it is forced to `NSVisualEffectState::Active` -
the default follows window-active state, and this window is deliberately never key, so it
would otherwise render in its lifeless inactive appearance forever.

The app lives in the menu bar (aleph icon); left-click opens the dashboard, right-click for
the menu. Tap **Right-⌘**, speak Hebrew, tap again.

First record loads the whisper model (a few seconds, once). DictaLM's first call pays a
~5s cold load, then translations are ~1-2s.

## Config (env)

| Var | Default | Purpose |
|---|---|---|
| `OZEN_HOTKEY` | `cmd_r` | `cmd_r` / `ctrl` / `f5` / `f6`; overrides the saved setting |
| `OLLAMA_MODEL` | `hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K` | translator + Hebrew polish |
| `OLLAMA_HOST` | `http://localhost:11434` | Ollama endpoint |
| `WHISPER_MODEL_PATH` | HF cache auto-resolve | ivrit GGML override |
| `OZEN_PROMPT` | Hebrew dev-speak bias (see `lib.rs`) | whisper initial prompt; empty disables |
| `OZEN_POLISH` | `1` | translate-off mode: `1` = DictaLM-polished Hebrew, `0` = raw transcript |
| `OZEN_DEBUG` | unset | write hotkey log to `/tmp/ozen-hotkey.log` |

## Requirements

- Ollama running with the DictaLM model pulled:
  `ollama pull hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K`
- ivrit-ai GGML whisper model in the HF cache (already present from the old build).

## Hebrew quality (2026-08-05 revival)

- Beam search (size 5) + Metal flash attention (whisper-rs 0.16) instead of greedy decode.
- Whisper initial prompt biases toward Hebrew dev-speak with Latin tech terms (`OZEN_PROMPT`).
- Hallucination gate on per-segment no-speech probability, ahead of the phrase blocklist.
- Translate OFF now pastes **polished Hebrew**: DictaLM fixes ASR errors, adds punctuation,
  and restores transliterated tech terms (קומיט -> commit). `OZEN_POLISH=0` for raw.

## v0.3.0 - toggle, cues, and the learning dictionary

**Toggle mode is the default.** Tap the hotkey to start, tap again to stop and paste. Hold
mode (the original push-to-talk) is still there in Settings. Because Right-⌘ is also a real
modifier, the event tap classifies each release: a toggle fires only on a *clean tap* - the
key went down and up alone, inside 400ms. Press Right-⌘+C and it stays a copy. A recording
you forget about auto-stops (default 180s, configurable).

**Cue tones.** Five motifs - start (rising), stop (single note), translating (quiet tick),
pasted (rising resolve), error (the only falling one). Volume and on/off in Settings, with
preview buttons.

**The dictionary learns itself.** The bottleneck is the Hebrew -> English step, not the ASR,
so every produced pair is counted into a co-occurrence table. A Hebrew token's rendering is
promoted to a forced hint only when it clears three bars: seen at least 3 times, a Dice
coefficient over 0.55, and a real *margin* over the runner-up. That last bar is the one that
matters - inside a single sentence every Hebrew word co-occurs with every English word
equally, so repetition alone can never separate them (there is a test pinning exactly this).
Promoted terms are injected, scoped to the sentence being translated, as a reference table.

Correcting an entry in the **יומן** tab is the supervised half: the corrected text is stored
as a locked term and as a retrievable exemplar, and similar future inputs get it back as a
worked example. Nothing you correct is ever overwritten by the counter.

**Statistics** on the home tab, all derived from the log (never stored as aggregates):
utterances, cumulative speech time, words/minute, words per utterance, letters per word,
median ASR and LLM latency - plus a quality block: expansion ratio (English words per Hebrew
word - a sagging ratio means dropped content), correction rate, and rejections broken down by
reason (short / silent / no text / ASR / LLM / paste).

Everything persists to `~/Library/Application Support/ai.orellius.ozen/`.

## Israeli-accented English

Speaking English is now a first-class path, not a degraded Hebrew one. `speech_lang` is
`auto` by default, so whisper detects per clip; the **script of the result** then picks the
repair, which means a clip that came out English skips translation entirely and gets an
accent repair pass instead.

The repair is prompt-driven and targets a specific, closed set - Hebrew's phoneme inventory
is missing several English contrasts outright, so the errors are predictable rather than
random:

| Hebrew L1 gap | How it surfaces in the transcript |
|---|---|
| no /w/ (labiovelar) | `ve` / `vant` / `vorld` for we / want / world |
| no /θ/ | `tink` / `sink` for think, `tree` for three |
| no /ð/ | `de` / `ze` for the, `dis` for this |
| no /æ/ vs /e/ | `bed` for bad, `men` for man |
| no /ɪ/ vs /iː/ | `sheep` for ship |
| final cluster reduction | `ask` for asked |
| `-ing` -> `-ink` | `workink` for working |
| no indefinite article | dropped a / an |

Whisper also gets a register-matched initial prompt per language mode - Hebrew dev-speak,
fast accented English dev-speak, or a short bilingual bias in auto. This matters more than it
sounds: a Hebrew-register prompt actively drags English clips toward Hebrew output.

Toggle it off in **הגדרות → שפה** if you want the raw transcript.

## Roadmap (not built yet)

- Editable preview HUD before paste (eyeball/edit the English first).
- Optional cloud fallback (Gemini/Soniox/ElevenLabs Scribe) behind a toggle for max fluency.
- VAD trim (Silero via ort) to cut leading/trailing silence before the whisper encode.
- Orb position + size persistence.
