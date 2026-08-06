// Review: the dashboard's front door, and the only place the app ever gets ground truth.
//
// Why this exists: every learning subsystem in Ozen - the term aligner, the mishearing table,
// the approved-exemplar few-shots - is fed by exactly one thing, a correction. Measured on the
// live log 2026-08-06: `corrected` was null on every single entry, so all three were built,
// tested, and completely dead. The editor was not broken; it was buried behind a tab, and a
// correction flow you have to go looking for is a correction flow that never runs.
//
// So this is a QUEUE, not a browser: one utterance at a time, keyboard-driven, with the
// consequence of each correction shown immediately. Correcting fifty utterances has to cost
// five minutes, or it does not happen at all.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cmd, type LogEntry, type Mishearing, type Term } from "../ipc";

interface Props {
  logs: LogEntry[];
  glossary: Term[];
  mishearings: Mishearing[];
  onCorrected: (at: number, corrected: string) => void;
  onToast: (msg: string, kind?: "error" | "ok") => void;
}

/** What the last correction taught, surfaced so the loop is visible rather than theoretical. */
interface Taught {
  terms: Term[];
  mishearings: Mishearing[];
}

export function Review({ logs, glossary, mishearings, onCorrected, onToast }: Props) {
  // Newest first: a fresh mistake is the one still in his head, and the one most likely to
  // repeat tomorrow. Ordering by age would start the queue with utterances he cannot remember.
  const queue = useMemo(
    () => logs.filter((e) => !e.corrected && e.english.trim()).sort((a, b) => b.at - a.at),
    [logs],
  );
  const [cursor, setCursor] = useState(0);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [taught, setTaught] = useState<Taught | null>(null);
  const box = useRef<HTMLTextAreaElement | null>(null);

  const entry: LogEntry | undefined = queue[Math.min(cursor, queue.length - 1)];

  useEffect(() => {
    setDraft(entry?.english ?? "");
  }, [entry?.at, entry?.english]);

  // Diff the learned tables across a save. The backend emits `glossary` and `mishearings` after
  // a correction lands, so what appeared between the two snapshots IS what this correction
  // taught - no guessing, and no second source of truth.
  const before = useRef<{ terms: Term[]; mishearings: Mishearing[] } | null>(null);
  useEffect(() => {
    const prev = before.current;
    if (!prev) return;
    const newTerms = glossary.filter((t) => !prev.terms.some((p) => p.he === t.he && p.en === t.en));
    const newMis = mishearings.filter(
      (m) => !prev.mishearings.some((p) => p.heard === m.heard && p.meant === m.meant),
    );
    before.current = null;
    if (newTerms.length || newMis.length) setTaught({ terms: newTerms, mishearings: newMis });
  }, [glossary, mishearings]);

  const advance = useCallback(() => {
    setTaught(null);
    setCursor((c) => (c + 1 >= queue.length ? Math.max(0, queue.length - 1) : c + 1));
  }, [queue.length]);

  const save = useCallback(async () => {
    if (!entry || busy) return;
    const text = draft.trim();
    if (!text) {
      onToast("תיקון ריק", "error");
      return;
    }
    if (text === entry.english.trim()) {
      // Approving unchanged text is not a correction and must never be written as one: the
      // learned tables would fill with confirmations of their own output.
      advance();
      return;
    }
    setBusy(true);
    before.current = { terms: glossary, mishearings };
    try {
      const ok = await cmd.correctEntry(entry.at, text);
      if (!ok) {
        onToast("השמירה נכשלה", "error");
        before.current = null;
        return;
      }
      onCorrected(entry.at, text);
      onToast("נשמר", "ok");
      // The queue drops this entry on the next render, so the cursor stays where it is and the
      // next utterance slides in under it.
    } catch (err: unknown) {
      before.current = null;
      onToast(`שמירה נכשלה: ${String(err)}`, "error");
    } finally {
      setBusy(false);
    }
  }, [advance, busy, draft, entry, glossary, mishearings, onCorrected, onToast]);

  // Keyboard is the point. Cmd+Enter saves, Enter alone approves and moves on, Esc restores the
  // original text - a queue you have to reach for the mouse in is a queue that stalls.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        void save();
      } else if (e.key === "Escape") {
        e.preventDefault();
        setDraft(entry?.english ?? "");
      } else if (e.key === "Enter" && document.activeElement !== box.current) {
        e.preventDefault();
        advance();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [advance, entry?.english, save]);

  const done = logs.filter((e) => e.corrected).length;

  if (!entry) {
    return (
      <section className="panel">
        <h2>אין מה לבדוק</h2>
        <p className="hint">
          כל מה שהוקלט כבר נבדק ({done} תיקונים). כל הכתבה חדשה תופיע כאן.
        </p>
      </section>
    );
  }

  return (
    <section className="panel review">
      <header className="review-head">
        <div>
          <strong>{queue.length}</strong> ממתינים · <strong>{done}</strong> תוקנו
        </div>
        <div className="hint">
          ⌘⏎ שמור תיקון · ⏎ תקין, הבא · esc שחזר
        </div>
      </header>

      <div className="review-card">
        <div className="he">{entry.hebrew}</div>
        <textarea
          ref={box}
          className="review-en"
          value={draft}
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
          dir={/[֐-׿]/.test(draft) ? "rtl" : "ltr"}
        />
        <div className="meta">
          {entry.mode} · {(entry.speech_ms / 1000).toFixed(1)}s דיבור ·{" "}
          {(entry.asr_ms / 1000).toFixed(1)}s תמלול · {(entry.llm_ms / 1000).toFixed(1)}s מודל
          {entry.confidence != null ? ` · ביטחון ${(entry.confidence * 100).toFixed(0)}%` : ""}
          {entry.hints_used ? ` · ${entry.hints_used} רמזים` : ""}
        </div>
      </div>

      {taught ? (
        <div className="taught">
          <strong>נלמד מהתיקון:</strong>
          {taught.mishearings.map((m) => (
            <span key={`m${m.heard}${m.meant}`} className="chip">
              שמע «{m.heard}» → {m.meant}
            </span>
          ))}
          {taught.terms.map((t) => (
            <span key={`t${t.he}${t.en}`} className="chip">
              {t.he} → {t.en}
            </span>
          ))}
        </div>
      ) : null}

      <div className="row">
        <button className="btn primary" disabled={busy} onClick={() => void save()}>
          שמור תיקון
        </button>
        <button className="btn" disabled={busy} onClick={advance}>
          תקין, הבא
        </button>
      </div>
    </section>
  );
}
