import { useState } from "react";
import { cmd, type CueName, type InputMode, type Settings, type Term } from "../ipc";

const CUES: { id: CueName; label: string }[] = [
  { id: "start", label: "התחלה" },
  { id: "stop", label: "סיום" },
  { id: "working", label: "מתרגם" },
  { id: "done", label: "הודבק" },
  { id: "error", label: "שגיאה" },
];

interface SettingsPageProps {
  settings: Settings;
  glossary: Term[];
  modelReady: boolean;
  onChange: (next: Settings) => void;
  onToast: (msg: string, kind?: "error" | "ok") => void;
}

export function SettingsPage({
  settings,
  glossary,
  modelReady,
  onChange,
  onToast,
}: SettingsPageProps) {
  // One writer for the whole struct: the tray flips `translate` on the same file, so partial
  // updates would race it. Every control sends the complete Settings object.
  const patch = (p: Partial<Settings>) => onChange({ ...settings, ...p });

  return (
    <>
      <h2 className="sec">הפעלה</h2>
      <div className="panel">
        <label className="field">
          <span>מצב</span>
          <select
            value={settings.input_mode}
            onChange={(e) => patch({ input_mode: e.target.value as InputMode })}
          >
            <option value="toggle">לחיצה להתחלה, לחיצה לסיום</option>
            <option value="hold">החזקה (push-to-talk)</option>
          </select>
        </label>
        <label className="field">
          <span>מקש</span>
          <select
            value={settings.hotkey}
            onChange={(e) => patch({ hotkey: e.target.value })}
          >
            <option value="cmd_r">⌘ ימני</option>
            <option value="ctrl">Control</option>
            <option value="f5">F5</option>
            <option value="f6">F6</option>
          </select>
        </label>
        <label className="field">
          <span>עצירה אוטומטית (שניות)</span>
          <input
            type="number"
            min={10}
            max={900}
            step={10}
            value={settings.max_seconds}
            onChange={(e) =>
              patch({ max_seconds: Math.max(10, Number(e.target.value) || 180) })
            }
          />
        </label>
        <p className="note">שינוי מקש נכנס לתוקף בהפעלה מחדש של האפליקציה.</p>
      </div>

      <h2 className="sec">פלט</h2>
      <div className="panel">
        <Switch
          checked={settings.translate}
          onChange={(v) => patch({ translate: v })}
          label="תרגום לאנגלית"
        />
        <Switch
          checked={settings.polish}
          onChange={(v) => patch({ polish: v })}
          label="ליטוש עברית (כשהתרגום כבוי)"
        />
        <Switch
          checked={settings.dictionary}
          onChange={(v) => patch({ dictionary: v })}
          label="מילון לומד"
        />
      </div>

      <h2 className="sec">צלילים</h2>
      <div className="panel">
        <Switch
          checked={settings.sounds}
          onChange={(v) => patch({ sounds: v })}
          label="צלילי שלב"
        />
        <label className="field">
          <span>עוצמה</span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={settings.sound_volume}
            onChange={(e) => patch({ sound_volume: Number(e.target.value) })}
          />
        </label>
        <div className="row cues">
          {CUES.map((c) => (
            <button
              key={c.id}
              className="btn btn-sm"
              onClick={() => void cmd.previewSound(c.id)}
            >
              {c.label}
            </button>
          ))}
        </div>
      </div>

      <h2 className="sec">
        המילון {glossary.length > 0 ? <span className="muted">({glossary.length})</span> : null}
      </h2>
      <Glossary glossary={glossary} onToast={onToast} />

      <h2 className="sec">הרשאות</h2>
      <div className="panel">
        <div className="row">
          <button
            className="btn"
            onClick={async () => {
              await cmd.requestAccessibility();
              onToast("אם נדרש, אשר ב'נגישות' והפעל מחדש את האפליקציה.");
            }}
          >
            נגישות
          </button>
          <button className="btn" onClick={() => void cmd.requestMicrophone()}>
            מיקרופון
          </button>
        </div>
      </div>

      <div className="models">
        <span className="model-line">
          תמלול: <b>ivrit-ai turbo</b>{" "}
          <em className={modelReady ? "ready" : undefined}>
            {modelReady ? "מוכן" : "טוען…"}
          </em>
        </span>
        <span className="model-line">
          תרגום: <b>DictaLM 3.0</b>
        </span>
      </div>
    </>
  );
}

function Switch({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="switch">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{label}</span>
    </label>
  );
}

function Glossary({
  glossary,
  onToast,
}: {
  glossary: Term[];
  onToast: (msg: string, kind?: "error" | "ok") => void;
}) {
  const [he, setHe] = useState("");
  const [en, setEn] = useState("");

  const add = async () => {
    if (!he.trim() || !en.trim()) {
      onToast("צריך מילה בעברית ותרגום");
      return;
    }
    await cmd.setTerm(he.trim(), en.trim());
    setHe("");
    setEn("");
  };

  return (
    <div className="panel">
      <div className="row">
        <input
          type="text"
          placeholder="מילה בעברית"
          value={he}
          onChange={(e) => setHe(e.target.value)}
        />
        <input
          type="text"
          dir="ltr"
          placeholder="translation"
          value={en}
          onChange={(e) => setEn(e.target.value)}
        />
        <button className="btn btn-sm" onClick={() => void add()}>
          הוסף
        </button>
      </div>
      <div className="terms">
        {glossary.length === 0 ? (
          <p className="empty">
            המילון ייבנה מעצמו ככל שתדבר. תיקון ביומן מוסיף מונח נעול.
          </p>
        ) : (
          glossary.map((t) => (
            <div key={t.he} className={t.locked ? "term is-locked" : "term"}>
              <span className="t-he">{t.he}</span>
              <span className="t-en" dir="ltr">
                {t.en}
              </span>
              <span className="t-n">{t.locked ? "נעול" : `×${t.hits}`}</span>
              <button className="fix" onClick={() => void cmd.forgetTerm(t.he)}>
                מחק
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
