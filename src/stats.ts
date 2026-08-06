// Pure derivations over the persisted log. No aggregate is ever stored: every number here is
// recomputed from the entries it summarises, so a stat can never drift away from its evidence.
import type { LogEntry, Rejection, RejectReason } from "./ipc";

export const words = (s: string): string[] =>
  s.trim().split(/\s+/).filter(Boolean);

export const letters = (s: string): number => (s.match(/\p{L}/gu) ?? []).length;

export function median(nums: number[]): number {
  if (nums.length === 0) return 0;
  const a = [...nums].sort((x, y) => x - y);
  const mid = a.length >> 1;
  return a.length % 2 ? a[mid]! : (a[mid - 1]! + a[mid]!) / 2;
}

export interface Stats {
  count: number;
  speechMs: number;
  /** Hebrew words per minute of recorded speech - your actual speaking rate. */
  wpm: number;
  wordsPerUtterance: number;
  lettersPerWord: number;
  asrMs: number;
  llmMs: number;
  /**
   * English words produced per Hebrew word consumed. Hebrew is the denser language, so healthy
   * output sits above ~1.1; a ratio that sags is the translator dropping content rather than
   * being concise, which is the failure mode that is invisible in a spot check.
   */
  expansion: number;
  corrections: number;
  correctionRate: number;
  rejections: number;
  rejectRate: number;
  byReason: Partial<Record<RejectReason, number>>;
}

export function computeStats(logs: LogEntry[], rejections: Rejection[]): Stats {
  const spoken = logs.filter((e) => e.speech_ms > 0);
  const speechMs = spoken.reduce((n, e) => n + e.speech_ms, 0);
  const heWords = spoken.reduce((n, e) => n + words(e.hebrew).length, 0);
  const heLetters = spoken.reduce((n, e) => n + letters(e.hebrew), 0);

  const translated = logs.filter((e) => e.mode === "translate" && e.english);
  const enWords = translated.reduce(
    (n, e) => n + words(e.corrected ?? e.english).length,
    0,
  );
  const tHeWords = translated.reduce((n, e) => n + words(e.hebrew).length, 0);

  const corrected = logs.filter((e) => e.corrected);
  const attempts = logs.length + rejections.length;

  const byReason: Partial<Record<RejectReason, number>> = {};
  for (const r of rejections) byReason[r.reason] = (byReason[r.reason] ?? 0) + 1;

  return {
    count: logs.length,
    speechMs,
    wpm: speechMs > 0 ? heWords / (speechMs / 60_000) : 0,
    wordsPerUtterance: spoken.length > 0 ? heWords / spoken.length : 0,
    lettersPerWord: heWords > 0 ? heLetters / heWords : 0,
    asrMs: median(logs.map((e) => e.asr_ms).filter((n) => n > 0)),
    llmMs: median(logs.map((e) => e.llm_ms).filter((n) => n > 0)),
    expansion: tHeWords > 0 ? enWords / tHeWords : 0,
    corrections: corrected.length,
    correctionRate: logs.length > 0 ? corrected.length / logs.length : 0,
    rejections: rejections.length,
    rejectRate: attempts > 0 ? rejections.length / attempts : 0,
    byReason,
  };
}

export function fmtDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} שנ׳`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}:${String(s % 60).padStart(2, "0")} דק׳`;
  return `${Math.floor(m / 60)}ש ${m % 60}ד`;
}

export const fmtMs = (ms: number): string =>
  ms >= 1000 ? `${(ms / 1000).toFixed(1)}ש` : `${Math.round(ms)}ms`;

export const fmtTime = (ms: number): string =>
  new Date(ms).toLocaleTimeString("he-IL", { hour: "2-digit", minute: "2-digit" });
