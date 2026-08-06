import { useState } from "react";
import { cmd, type LogEntry } from "../ipc";
import { fmtDuration, fmtMs, fmtTime } from "../stats";

interface LogsProps {
  logs: LogEntry[];
  onCorrected: (at: number, corrected: string) => void;
  onCleared: () => void;
  onToast: (msg: string, kind?: "error" | "ok") => void;
}

export function Logs({ logs, onCorrected, onCleared, onToast }: LogsProps) {
  return (
    <>
      <div className="listbar">
        <span className="muted">{logs.length} רשומות</span>
        <button
          className="btn btn-sm"
          onClick={async () => {
            await cmd.clearLogs();
            onCleared();
          }}
        >
          נקה יומן
        </button>
      </div>
      <div className="list">
        {logs.length === 0 ? (
          <p className="empty">עוד לא נאמר כלום.</p>
        ) : (
          // Newest first; the backend appends chronologically.
          [...logs]
            .reverse()
            .map((it) => (
              <Card key={it.at} entry={it} onCorrected={onCorrected} onToast={onToast} />
            ))
        )}
      </div>
    </>
  );
}

interface CardProps {
  entry: LogEntry;
  onCorrected: (at: number, corrected: string) => void;
  onToast: (msg: string, kind?: "error" | "ok") => void;
}

function Card({ entry, onCorrected, onToast }: CardProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(entry.corrected ?? entry.english);

  const meta = [fmtTime(entry.at), fmtDuration(entry.speech_ms)];
  if (entry.asr_ms > 0) meta.push(`ASR ${fmtMs(entry.asr_ms)}`);
  if (entry.llm_ms > 0) meta.push(`LLM ${fmtMs(entry.llm_ms)}`);
  if (entry.lang) meta.push(entry.lang);
  // Decoder confidence is only worth surfacing when it is BAD - a number beside every line
  // becomes wallpaper, and the point is to make a guessed word stand out.
  if (entry.confidence > 0 && entry.confidence < 0.6)
    meta.push(`ביטחון ${Math.round(entry.confidence * 100)}%`);
  if (entry.auto_fixed > 0) meta.push(`תוקנו ${entry.auto_fixed}`);
  if (entry.hints_used > 0) meta.push(`${entry.hints_used} רמזים`);
  if (entry.corrected) meta.push("תוקן");

  // The correction editor is this app's only source of supervised truth, so it is one click
  // from every entry and commits on Cmd+Enter.
  const commit = async () => {
    const text = draft.trim();
    if (!text) return;
    const ok = await cmd.correctEntry(entry.at, text);
    if (!ok) {
      onToast("לא נמצאה הרשומה");
      return;
    }
    onCorrected(entry.at, text);
    setEditing(false);
    onToast("נשמר - המילון עודכן", "ok");
  };

  return (
    <div className={entry.corrected ? "card is-fixed" : "card"}>
      <div className="he">{entry.hebrew}</div>
      <div className="en">{entry.corrected ?? entry.english}</div>
      <div className="meta">
        {meta.join(" · ")}
        <button
          className="fix"
          title="כתוב את הניסוח הנכון - הוא ייכנס למילון"
          onClick={() => setEditing((v) => !v)}
        >
          {editing ? "סגור" : "תקן"}
        </button>
      </div>
      {editing ? (
        <div className="fixbox">
          <textarea
            dir="ltr"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void commit();
              if (e.key === "Escape") setEditing(false);
            }}
          />
          <div className="row">
            <button className="btn btn-sm" onClick={() => void commit()}>
              שמור ולמד
            </button>
            <button className="btn btn-sm" onClick={() => setEditing(false)}>
              בטל
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
