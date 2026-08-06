import type { LogEntry, Rejection, RejectReason, Settings, Term } from "../ipc";
import { computeStats, fmtDuration, fmtMs } from "../stats";
import { Tile } from "./Tile";

const HOTKEY_LABEL: Record<string, string> = {
  cmd_r: "⌘ ימני",
  ctrl: "Control",
  f5: "F5",
  f6: "F6",
};

const REASON_LABEL: Record<RejectReason, string> = {
  short: "קצר מדי",
  silent: "שקט",
  empty: "לא זוהה טקסט",
  asr: "כשל תמלול",
  llm: "כשל תרגום",
  paste: "כשל הדבקה",
};

interface HomeProps {
  settings: Settings;
  logs: LogEntry[];
  rejections: Rejection[];
  glossary: Term[];
}

export function Home({ settings, logs, rejections, glossary }: HomeProps) {
  const s = computeStats(logs, rejections);
  const key = HOTKEY_LABEL[settings.hotkey] ?? settings.hotkey;

  return (
    <>
      <p className="hint">
        {settings.input_mode === "hold"
          ? `החזק ${key} ודבר · שחרר כדי להדביק`
          : `הקש ${key} כדי להתחיל · הקש שוב כדי לסיים ולהדביק`}
      </p>

      <div className="stats">
        <Tile label="הכתבות" value={s.count} sub={`${s.corrections} תוקנו`} />
        <Tile label="זמן דיבור" value={fmtDuration(s.speechMs)} sub="מצטבר" />
        <Tile label="קצב" value={Math.round(s.wpm)} sub="מילים לדקה" />
        <Tile
          label="אורך הכתבה"
          value={s.wordsPerUtterance.toFixed(1)}
          sub="מילים בממוצע"
        />
        <Tile
          label="אותיות למילה"
          value={s.lettersPerWord.toFixed(2)}
          sub="בעברית"
        />
        <Tile label="תמלול" value={fmtMs(s.asrMs)} sub="חציון" />
        <Tile label="תרגום" value={fmtMs(s.llmMs)} sub="חציון" />
        <Tile label="מילון" value={glossary.length} sub="מונחים נלמדו" />
      </div>

      <h2 className="sec">איפה נופלת האיכות</h2>
      <div className="stats stats-sm">
        <Tile
          label="יחס הרחבה"
          value={s.expansion > 0 ? s.expansion.toFixed(2) : "-"}
          sub="מילות EN לכל HE"
        />
        <Tile
          label="שיעור תיקון"
          value={`${Math.round(s.correctionRate * 100)}%`}
          sub="תיקנת ידנית"
        />
        <Tile
          label="נדחו"
          value={s.rejections}
          sub={`${Math.round(s.rejectRate * 100)}% מהניסיונות`}
        />
        {(Object.entries(s.byReason) as [RejectReason, number][])
          .sort((a, b) => b[1] - a[1])
          .map(([reason, n]) => (
            <Tile key={reason} label={REASON_LABEL[reason] ?? reason} value={n} sub="דחיות" />
          ))}
      </div>

      <p className="note">
        תיקון שאתה עושה ביומן נכנס למילון הלומד ומוזרק לתרגום הבא של אותם מונחים.
      </p>
    </>
  );
}
