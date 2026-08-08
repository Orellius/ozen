// The dashboard shell: owns the snapshot, subscribes to the backend's events, routes the tabs.
// The dictation loop itself (capture -> ASR -> translate -> paste -> learn) runs entirely in
// Rust; nothing here drives it, this view only reflects it and edits settings.
import { useCallback, useEffect, useRef, useState } from "react";
import { cmd, on, type PipelineState, type Settings, type Snapshot } from "./ipc";
import { Home } from "./components/Home";
import { Review } from "./components/Review";
import { Logs } from "./components/Logs";
import { SettingsPage } from "./components/SettingsPage";

type Tab = "review" | "home" | "logs" | "settings";

// Review is FIRST and default: it is the only surface that produces ground truth, and a
// correction queue nobody lands on is a correction queue nobody uses (measured - `corrected`
// was null on all 92 logged utterances).
const TABS: { id: Tab; label: string }[] = [
  { id: "review", label: "בדיקה" },
  { id: "home", label: "בית" },
  { id: "logs", label: "יומן" },
  { id: "settings", label: "הגדרות" },
];

const STATE_TEXT: Record<PipelineState, string> = {
  idle: "מוכן",
  recording: "מקליט…",
  transcribing: "מתמלל…",
  translating: "מתרגם…",
  error: "שגיאה",
};

const EMPTY: Snapshot = {
  recording: false,
  processing: false,
  model_ready: false,
  model: "",
  settings: {
    input_mode: "toggle",
    hotkey: "cmd_r",
    translate: true,
    polish: true,
    sounds: true,
    sound_volume: 0.35,
    dictionary: true,
    max_seconds: 180,
    speech_lang: "auto",
    accent_repair: true,
  },
  logs: [],
  rejections: [],
  glossary: [],
  mishearings: [],
  night_summary: "",
};

interface Toast {
  msg: string;
  kind: "error" | "ok";
}

export function App() {
  const [tab, setTab] = useState<Tab>("review");
  const [snap, setSnap] = useState<Snapshot>(EMPTY);
  const [state, setState] = useState<PipelineState>("idle");
  const [toast, setToast] = useState<Toast | null>(null);
  const [axError, setAxError] = useState<string | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  const showToast = useCallback((msg: string, kind: "error" | "ok" = "error") => {
    setToast({ msg, kind });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 4500);
  }, []);

  useEffect(() => {
    let alive = true;
    const unlisteners: Promise<() => void>[] = [
      on("state", (s) => setState(s)),
      on("result", (entry) =>
        setSnap((p) => ({ ...p, logs: [...p.logs, entry] })),
      ),
      on("error", (msg) => showToast(msg)),
      on("model-ready", (ok) => setSnap((p) => ({ ...p, model_ready: ok }))),
      on("settings", (settings) => setSnap((p) => ({ ...p, settings }))),
      on("glossary", (glossary) => setSnap((p) => ({ ...p, glossary }))),
      on("mishearings", (mishearings) => setSnap((p) => ({ ...p, mishearings }))),
      on("needs-accessibility", (msg) => setAxError(msg)),
    ];

    cmd
      .getState()
      .then((s) => {
        if (!alive) return;
        setSnap(s);
        setState(s.processing ? "transcribing" : s.recording ? "recording" : "idle");
      })
      .catch((err: unknown) => showToast(`טעינה נכשלה: ${String(err)}`));

    return () => {
      alive = false;
      for (const u of unlisteners) void u.then((fn) => fn());
    };
  }, [showToast]);

  const saveSettings = useCallback((next: Settings) => {
    setSnap((p) => ({ ...p, settings: next }));
    void cmd.saveSettings(next);
  }, []);

  const onCorrected = useCallback((at: number, corrected: string) => {
    setSnap((p) => ({
      ...p,
      logs: p.logs.map((e) => (e.at === at ? { ...e, corrected } : e)),
    }));
  }, []);

  return (
    <main className="app">
      <header className="topbar">
        <span className="dot" data-state={state} />
        <h1>Ozen</h1>
        <span className="status">{STATE_TEXT[state]}</span>
      </header>

      <nav className="tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={t.id === tab ? "tab is-active" : "tab"}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>

      {axError ? (
        <section className="panel warn">
          <p className="perms-msg">נדרשת הרשאת נגישות למקש הקיצור: {axError}</p>
          <div className="row">
            <button
              className="btn"
              onClick={async () => {
                await cmd.requestAccessibility();
                showToast("אם נדרש, אשר ב'נגישות' והפעל מחדש את האפליקציה.");
              }}
            >
              אפשר נגישות
            </button>
            <button className="btn" onClick={() => void cmd.requestMicrophone()}>
              אפשר מיקרופון
            </button>
          </div>
        </section>
      ) : null}

      <div className="page">
        {tab === "review" ? (
          <Review
            logs={snap.logs}
            glossary={snap.glossary}
            mishearings={snap.mishearings}
            onCorrected={onCorrected}
            onToast={showToast}
          />
        ) : null}
        {tab === "home" ? (
          <Home
            settings={snap.settings}
            logs={snap.logs}
            rejections={snap.rejections}
            glossary={snap.glossary}
            nightSummary={snap.night_summary}
          />
        ) : null}
        {tab === "logs" ? (
          <Logs
            logs={snap.logs}
            onCorrected={onCorrected}
            onCleared={() => setSnap((p) => ({ ...p, logs: [] }))}
            onToast={showToast}
          />
        ) : null}
        {tab === "settings" ? (
          <SettingsPage
            settings={snap.settings}
            glossary={snap.glossary}
            mishearings={snap.mishearings}
            modelReady={snap.model_ready}
            onChange={saveSettings}
            onToast={showToast}
          />
        ) : null}
      </div>

      {toast ? (
        <div className="toast" data-kind={toast.kind}>
          {toast.msg}
        </div>
      ) : null}
    </main>
  );
}
