// The typed edge of the Tauri boundary. Every shape here mirrors a #[derive(Serialize)] struct
// in src-tauri/src (store.rs and lib.rs); nothing else in the UI is allowed to call invoke() or
// listen() directly, so a rename on the Rust side has exactly one place to break.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type InputMode = "toggle" | "hold";
/** "repair" = the clip was already English and got accent repair instead of translation. */
export type OutputMode = "translate" | "polish" | "repair" | "raw";
export type SpeechLang = "he" | "en" | "auto";
export type PipelineState =
  | "idle"
  | "recording"
  | "transcribing"
  | "translating"
  | "error";
export type CueName = "start" | "stop" | "working" | "done" | "error";
/// Mirrors store.rs Rejection.reason.
export type RejectReason = "short" | "silent" | "empty" | "asr" | "llm" | "paste";

/** store.rs :: Settings */
export interface Settings {
  input_mode: InputMode;
  hotkey: string;
  translate: boolean;
  polish: boolean;
  sounds: boolean;
  sound_volume: number;
  dictionary: boolean;
  max_seconds: number;
  speech_lang: SpeechLang;
  accent_repair: boolean;
}

/** store.rs :: LogEntry */
export interface LogEntry {
  at: number;
  hebrew: string;
  english: string;
  corrected: string | null;
  speech_ms: number;
  asr_ms: number;
  llm_ms: number;
  mode: OutputMode;
  /** What whisper decided the clip was, and how sure the decoder was (0..1). */
  lang: string;
  confidence: number;
  /** How much the learned dictionary contributed - hints given, mishearings auto-repaired. */
  hints_used: number;
  auto_fixed: number;
  /** He said roughly this again shortly afterwards - a free hint that this paste was wrong. */
  redictated: boolean;
}

/** store.rs :: Mishearing - a word the ASR reliably gets wrong, and what was meant. */
export interface Mishearing {
  heard: string;
  meant: string;
  hits: number;
  locked: boolean;
  last_at: number;
}

/** store.rs :: Rejection */
export interface Rejection {
  at: number;
  reason: RejectReason;
}

/** store.rs :: Term */
export interface Term {
  he: string;
  en: string;
  hits: number;
  locked: boolean;
  last_at: number;
}

/** lib.rs :: Snapshot */
export interface Snapshot {
  recording: boolean;
  processing: boolean;
  model_ready: boolean;
  model: string;
  settings: Settings;
  logs: LogEntry[];
  rejections: Rejection[];
  glossary: Term[];
  mishearings: Mishearing[];
  /** One line from the last overnight improvement pass; empty until one has run. */
  night_summary: string;
}

export const cmd = {
  getState: (): Promise<Snapshot> => invoke("get_state"),
  saveSettings: (settings: Settings): Promise<void> =>
    invoke("save_settings", { settings }),
  setTranslate: (enabled: boolean): Promise<void> =>
    invoke("set_translate", { enabled }),
  previewSound: (cue: CueName): Promise<void> => invoke("preview_sound", { cue }),
  correctEntry: (at: number, corrected: string): Promise<boolean> =>
    invoke("correct_entry", { at, corrected }),
  setTerm: (he: string, en: string): Promise<void> => invoke("set_term", { he, en }),
  forgetTerm: (he: string): Promise<void> => invoke("forget_term", { he }),
  forgetMishearing: (heard: string): Promise<void> =>
    invoke("forget_mishearing", { heard }),
  clearLogs: (): Promise<void> => invoke("clear_logs"),
  requestAccessibility: (): Promise<boolean> => invoke("request_accessibility"),
  requestMicrophone: (): Promise<void> => invoke("request_microphone"),
};

/** Every event lib.rs emits, with its payload type. */
export interface Events {
  state: PipelineState;
  level: number;
  result: LogEntry;
  error: string;
  "model-ready": boolean;
  settings: Settings;
  glossary: Term[];
  mishearings: Mishearing[];
  "needs-accessibility": string;
}

export function on<K extends keyof Events>(
  name: K,
  handler: (payload: Events[K]) => void,
): Promise<UnlistenFn> {
  return listen<Events[K]>(name, (e) => handler(e.payload));
}
