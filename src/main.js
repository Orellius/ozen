// Dashboard only. The push-to-talk loop (capture -> ASR -> translate -> paste) runs natively in Rust;
// this view just reflects state via events and exposes settings + permission prompts + history.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
let toastTimer = null;

function showToast(msg) {
  const t = $("toast");
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.hidden = true), 4500);
}

const STATE_TEXT = {
  idle: "מוכן",
  recording: "מקליט…",
  transcribing: "מתמלל…",
  translating: "מתרגם…",
  error: "שגיאה",
};
function setDot(state) {
  $("dot").dataset.state = state;
  $("status").textContent = STATE_TEXT[state] || "";
}

function hotkeyLabel(hk) {
  return { cmd_r: "⌘ ימני", ctrl: "Control", f5: "F5", f6: "F6" }[hk] || hk;
}

function fmtTime(ms) {
  return new Date(ms).toLocaleTimeString("he-IL", { hour: "2-digit", minute: "2-digit" });
}

function card(it) {
  const el = document.createElement("div");
  el.className = "card";
  const he = document.createElement("div");
  he.className = "he";
  he.textContent = it.hebrew;
  const en = document.createElement("div");
  en.className = "en";
  en.textContent = it.english;
  const meta = document.createElement("div");
  meta.className = "meta";
  meta.textContent = fmtTime(it.at);
  el.append(he, en, meta);
  return el;
}

function renderHistory(items) {
  const list = $("list");
  list.innerHTML = "";
  if (!items.length) {
    const p = document.createElement("p");
    p.className = "empty";
    p.textContent = "עוד לא נאמר כלום. החזק את המקש ודבר.";
    list.appendChild(p);
    return;
  }
  for (const it of items) list.appendChild(card(it));
}

function prependHistory(it) {
  const list = $("list");
  const empty = list.querySelector(".empty");
  if (empty) empty.remove();
  list.prepend(card(it));
}

function setModelReady(ready) {
  const el = $("asr-state");
  el.textContent = ready ? "מוכן" : "טוען…";
  el.classList.toggle("ready", !!ready);
}

async function refresh() {
  const s = await invoke("get_state");
  $("hotkey").textContent = hotkeyLabel(s.hotkey);
  $("translate").checked = s.translate_enabled;
  setModelReady(s.model_ready);
  setDot(s.processing ? "transcribing" : s.recording ? "recording" : "idle");
  renderHistory(s.history);
}

window.addEventListener("DOMContentLoaded", async () => {
  $("translate").addEventListener("change", (e) =>
    invoke("set_translate", { enabled: e.target.checked }),
  );
  $("grant-ax").addEventListener("click", async () => {
    await invoke("request_accessibility");
    showToast("אם נדרש, אשר ב'נגישות' והפעל מחדש את האפליקציה.");
  });
  $("grant-mic").addEventListener("click", () => invoke("request_microphone"));

  await listen("state", (e) => setDot(e.payload));
  await listen("result", (e) => prependHistory(e.payload));
  await listen("error", (e) => showToast(e.payload));
  await listen("model-ready", (e) => setModelReady(e.payload));
  await listen("translate", (e) => ($("translate").checked = e.payload));
  await listen("needs-accessibility", (e) => {
    $("perms").hidden = false;
    $("perms-msg").textContent = "נדרשת הרשאת נגישות למקש הקיצור: " + e.payload;
  });

  refresh().catch((err) => showToast("טעינה נכשלה: " + err));
});
