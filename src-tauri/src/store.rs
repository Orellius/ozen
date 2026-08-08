//! store.rs: everything that must survive a restart - settings, the utterance log, and the
//!   self-building translation dictionary.
//! Public surface: Store::load(dir), settings/save_settings, append_log/logs/correct,
//!   note_rejection/rejections, observe(hebrew, english), glossary/set_term/forget_term,
//!   hints_for(hebrew) -> Hints.
//! Why this file: the accuracy bottleneck is the Hebrew -> English step, and the only cheap
//!   source of ground truth about Orel's own vocabulary is Orel's own past utterances. Every
//!   pair the pipeline produces is counted; pairs that recur become forced renderings, and
//!   pairs he corrects by hand become locked exemplars. Nothing here calls a model.
//! NOT responsible for: translating, transcribing, or deciding when to inject the hints
//!   (translate.rs consumes Hints; lib.rs sequences).
//! Test strategy: feed the same (hebrew, english) pair 3x through observe() and assert the
//!   aligned term appears in hints_for() for a sentence containing that Hebrew token; assert a
//!   competing rendering seen once does NOT.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// A pair must be seen this many times AND score this Dice coefficient before it is trusted
/// enough to be forced into a prompt. Below either bar it stays a private count and changes
/// nothing - the aligner is silent until it is sure.
///
/// Dice, not raw share: within a single sentence every Hebrew word co-occurs with every English
/// word equally, so a share-of-row test can never separate the real pairing from its neighbours
/// (measured - it promoted nothing at all). Dividing by how often EACH side appears overall is
/// what makes the true pair win: "commit" shows up almost only beside "קומיט", while "open"
/// shows up beside everything, so only the first scores near 1.0.
const MIN_HITS: u32 = 3;
const MIN_DICE: f32 = 0.55;
/// How far the winner must beat the runner-up. Zero margin means the evidence cannot tell the
/// two candidates apart, however many times it was seen (see `best_pair`).
const MIN_MARGIN: f32 = 0.15;
/// Prompt budget guards: an unbounded hint block would eventually swamp the instruction.
const MAX_TERM_HINTS: usize = 24;
const MAX_EXEMPLARS: usize = 3;
/// Fuzzy-match floor for retrieving a past correction as a few-shot example.
const MIN_OVERLAP: f32 = 0.18;
/// Ceilings. The log is the stats source, so it is generous; the aligner table is pruned.
const LOG_CAP: usize = 4000;
const REJECT_CAP: usize = 1000;
const ALIGN_CAP: usize = 4000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "toggle" (tap on, tap off) or "hold" (the original push-to-talk).
    pub input_mode: String,
    pub hotkey: String,
    pub translate: bool,
    pub polish: bool,
    pub sounds: bool,
    pub sound_volume: f32,
    /// Master switch for the learned dictionary. Off = the model sees no hints at all.
    pub dictionary: bool,
    /// Auto-stop for toggle mode, seconds. A forgotten toggle must not record forever.
    pub max_seconds: u64,
    /// What whisper is told to expect: "he", "en", or "auto" (detect per clip).
    pub speech_lang: String,
    /// Repair Hebrew-accented English before pasting (see translate.rs REPAIR_PROMPT).
    pub accent_repair: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input_mode: "toggle".to_string(),
            hotkey: "cmd_r".to_string(),
            translate: true,
            polish: true,
            sounds: true,
            sound_volume: 0.35,
            dictionary: true,
            max_seconds: 180,
            speech_lang: "auto".to_string(),
            accent_repair: true,
        }
    }
}

/// One completed utterance. `corrected` is Orel's own fix and is the only ground truth here.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogEntry {
    pub at: u64,
    pub hebrew: String,
    pub english: String,
    pub corrected: Option<String>,
    pub speech_ms: u64,
    pub asr_ms: u64,
    pub llm_ms: u64,
    /// "translate" | "polish" | "repair" | "raw"
    pub mode: String,
    /// What whisper decided the clip was ("he" / "en"), and how sure the decoder was
    /// (mean per-token probability, 0..1). A low number here is the tell that a word was
    /// guessed - it is the difference between "the output was odd" and knowing WHY.
    pub lang: String,
    pub confidence: f32,
    /// How many learned hints the model was given for this utterance, and how many confirmed
    /// mishearings were repaired before it ran. Without these the dictionary is unfalsifiable:
    /// there would be no way to tell whether it is earning its keep or just costing tokens.
    pub hints_used: usize,
    pub auto_fixed: usize,
    /// Set on an entry when the SPEAKER SAID ROUGHLY THE SAME THING AGAIN shortly afterwards.
    ///
    /// This is the only negative label the app can collect without asking for one. The correction
    /// queue has produced 5 labels in 267 utterances, and the Accessibility route that would watch
    /// him edit the pasted text is unavailable because he dictates into a terminal, which exposes
    /// no editable text. Repeating yourself, however, is observable from inside the app and costs
    /// him nothing.
    ///
    /// It is a CANDIDATE, never a verdict: repeating a sentence can also mean he added a thought.
    /// Nothing acts on this field at runtime; the night pass weighs it against a strong model's
    /// opinion before anything is learned from it.
    pub redictated: bool,
}

impl Default for LogEntry {
    fn default() -> Self {
        Self {
            at: 0,
            hebrew: String::new(),
            english: String::new(),
            corrected: None,
            speech_ms: 0,
            asr_ms: 0,
            llm_ms: 0,
            mode: "translate".to_string(),
            lang: String::new(),
            confidence: 0.0,
            hints_used: 0,
            auto_fixed: 0,
            redictated: false,
        }
    }
}

/// A clip that never reached the model. Counting these is how the dashboard shows whether
/// accuracy is leaking at capture (too short, silent) or at ASR (hallucination, error).
#[derive(Clone, Serialize, Deserialize)]
pub struct Rejection {
    pub at: u64,
    /// "short" | "silent" | "empty" | "asr" | "llm" | "paste"
    pub reason: String,
}

/// One forced rendering: whenever `he` shows up in the Hebrew, the model is told to write `en`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Term {
    pub he: String,
    pub en: String,
    pub hits: u32,
    /// Locked terms come from Orel (a correction or a manual edit) and the aligner never
    /// overwrites them - his word outranks any amount of counting.
    pub locked: bool,
    pub last_at: u64,
}

impl Default for Term {
    fn default() -> Self {
        Self {
            he: String::new(),
            en: String::new(),
            hits: 0,
            locked: false,
            last_at: 0,
        }
    }
}

/// A corrected pair, retrieved by word overlap and shown to the model as a worked example.
#[derive(Clone, Serialize, Deserialize)]
pub struct Exemplar {
    pub hebrew: String,
    pub english: String,
    pub at: u64,
}

/// A word the ASR reliably gets wrong, and what was actually said.
///
/// This is the defect class nothing else in the pipeline can see: the transcript is a real
/// word, spelled correctly, in a grammatical sentence - it is simply the WRONG word ("comic
/// push" for "commit and push", observed live 2026-08-06). Spell check passes it, the LLM
/// cleanup passes it, and the only evidence that anything is wrong is that it sounds like
/// something the speaker actually says.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Mishearing {
    pub heard: String,
    pub meant: String,
    pub hits: u32,
    /// Confirmed by Orel correcting it. Locked rules are applied SILENTLY; unconfirmed ones
    /// are only ever offered to the model as a suggestion, because silently rewriting a rare
    /// but correct word is worse than leaving a wrong one.
    pub locked: bool,
    pub last_at: u64,
}

/// What `scripts/night-pass.py` leaves behind for the app to pick up. Deliberately a plain data
/// file rather than a second writer of `dictionary.json`: the app owns that file and writes it
/// whole, so an outside process editing it would silently lose whatever the app held in memory.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct Proposals {
    pub exemplars: Vec<ProposedExemplar>,
    pub mishearings: Vec<ProposedMishearing>,
    pub summary: String,
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct ProposedExemplar {
    pub hebrew: String,
    pub english: String,
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct ProposedMishearing {
    pub heard: String,
    pub meant: String,
}

impl Default for Mishearing {
    fn default() -> Self {
        Self {
            heard: String::new(),
            meant: String::new(),
            hits: 0,
            locked: false,
            last_at: 0,
        }
    }
}

/// An unconfirmed sound-alike hit found in the current transcript.
#[derive(Clone, Serialize)]
pub struct Suspect {
    pub heard: String,
    pub meant: String,
    pub score: f32,
}

/// What translate.rs injects for one specific input.
#[derive(Clone, Default, Serialize)]
pub struct Hints {
    pub terms: Vec<Term>,
    pub exemplars: Vec<Exemplar>,
    pub suspects: Vec<Suspect>,
}

impl Hints {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.exemplars.is_empty() && self.suspects.is_empty()
    }
    pub fn count(&self) -> usize {
        self.terms.len() + self.exemplars.len() + self.suspects.len()
    }
}

/// he_token -> (en_token -> count). The raw co-occurrence table the aligner reasons over.
type AlignTable = HashMap<String, HashMap<String, u32>>;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Dictionary {
    terms: Vec<Term>,
    exemplars: Vec<Exemplar>,
    align: AlignTable,
    /// How many utterances each side appeared in at all - the denominators Dice needs.
    he_seen: HashMap<String, u32>,
    en_seen: HashMap<String, u32>,
    mishearings: Vec<Mishearing>,
    /// Every word Orel has been observed to actually use, with a count. This is what a
    /// suspected mishearing is matched AGAINST - a generic English dictionary would be useless
    /// here, because the whole question is "which of HIS words does this sound like".
    vocab: HashMap<String, u32>,
}

/// Seeds the vocabulary so the very first session can already catch mishearings, before any
/// corrections exist. Same terms the whisper prompts bias toward - one list, two consumers.
const SEED_VOCAB: &[&str] = &[
    "commit", "push", "branch", "merge", "rebase", "repo", "repository", "terminal", "build",
    "deploy", "debug", "test", "tests", "refactor", "function", "endpoint", "server", "client",
    "frontend", "backend", "database", "migration", "release", "revert", "stash", "clone",
    "pull", "request", "review", "compile", "runtime", "package", "import", "export", "module",
    "component", "config", "schema", "query", "cache", "buffer", "thread", "async", "await",
    "error", "warning", "logging", "screenshot", "install", "update", "upgrade", "rollback",
    "rust", "typescript", "python", "swift", "tauri", "react", "cargo", "github", "docker",
    "ollama", "claude", "whisper", "ozen", "sadna", "studio", "xcode", "keychain", "finder",
];

/// A rule must be seen this often before it is even worth mentioning to the model.
const MISHEARING_MIN_HITS: u32 = 2;
/// Two bars, because two situations with very different evidence:
///
/// SUSPECT is the unprompted guess - nobody has told us anything, we are inferring from sound
/// alone that a correctly-spelled word in a grammatical sentence is the wrong word. A false
/// positive here puts a wrong suggestion in front of the model, so it is set high.
///
/// LEARN applies when Orel has ALREADY given the answer by correcting the entry. The only
/// question left is whether he fixed a mishearing (generalises to every future utterance) or
/// rephrased (must never generalise). Distinguishing those two needs a much lower bar than
/// finding a mishearing unaided - and the live case proves it: "comic" -> "commit" scores
/// 0.62, well under the suspicion bar, yet it is unmistakably a mishearing.
const SUSPECT_MIN_SCORE: f32 = 0.74;
const LEARN_MIN_SCORE: f32 = 0.55;
/// Vocabulary entries not seen in this long lose half their weight, so an early wrong pairing
/// fades instead of outranking current usage forever.
const DECAY_AFTER_MS: u64 = 30 * 24 * 60 * 60 * 1000;

pub struct Store {
    dir: PathBuf,
    settings: Mutex<Settings>,
    log: Mutex<Vec<LogEntry>>,
    rejections: Mutex<Vec<Rejection>>,
    dict: Mutex<Dictionary>,
}

impl Store {
    pub fn load(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let mut dict: Dictionary = read_json(&dir.join("dictionary.json")).unwrap_or_default();
        decay(&mut dict);
        let store = Self {
            settings: Mutex::new(read_json(&dir.join("settings.json")).unwrap_or_default()),
            log: Mutex::new(read_json(&dir.join("log.json")).unwrap_or_default()),
            rejections: Mutex::new(read_json(&dir.join("rejections.json")).unwrap_or_default()),
            dict: Mutex::new(dict),
            dir,
        };
        store
    }

    // ---- settings ----

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn save_settings(&self, next: Settings) {
        if let Ok(mut s) = self.settings.lock() {
            *s = next;
            write_json(&self.dir.join("settings.json"), &*s);
        }
    }

    // ---- log + rejections ----

    pub fn append_log(&self, entry: LogEntry) {
        if let Ok(mut log) = self.log.lock() {
            // Before the new entry lands, ask whether it is a REPEAT of the one before it. If it
            // is, the previous paste is the thing he was not satisfied with - a free negative
            // label, collected without a queue, a hotkey, or a single keystroke from him.
            if let Some(prev) = log.last_mut() {
                if is_redictation(prev, &entry) {
                    prev.redictated = true;
                }
            }
            log.push(entry);
            let overflow = log.len().saturating_sub(LOG_CAP);
            if overflow > 0 {
                log.drain(0..overflow);
            }
            write_json(&self.dir.join("log.json"), &*log);
        }
    }

    pub fn logs(&self) -> Vec<LogEntry> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn clear_logs(&self) {
        if let Ok(mut log) = self.log.lock() {
            log.clear();
            write_json(&self.dir.join("log.json"), &*log);
        }
    }

    pub fn note_rejection(&self, reason: &str, at: u64) {
        if let Ok(mut r) = self.rejections.lock() {
            r.push(Rejection {
                at,
                reason: reason.to_string(),
            });
            let overflow = r.len().saturating_sub(REJECT_CAP);
            if overflow > 0 {
                r.drain(0..overflow);
            }
            write_json(&self.dir.join("rejections.json"), &*r);
        }
    }

    pub fn rejections(&self) -> Vec<Rejection> {
        self.rejections.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Orel edited a past translation. That correction is ground truth: it is stored on the
    /// entry, kept as a retrievable exemplar, and its aligned terms are locked.
    pub fn correct(&self, at: u64, corrected: &str) -> bool {
        let Ok(mut log) = self.log.lock() else {
            return false;
        };
        let Some(entry) = log.iter_mut().find(|e| e.at == at) else {
            return false;
        };
        entry.corrected = Some(corrected.to_string());
        let hebrew = entry.hebrew.clone();
        let produced = entry.english.clone();
        let was_translation = entry.mode == "translate";
        write_json(&self.dir.join("log.json"), &*log);
        drop(log);

        // A correction feeds three different learners, and the split matters:
        //   - what he MEANT overall  -> an exemplar, retrievable for similar future input
        //   - which he->en RENDERING -> the aligner, but only for actual translations
        //   - which words were MISHEARD -> the phonetic table, for every mode
        // The exemplar is counted ONCE, deliberately: replaying one sentence N times is the
        // degenerate input the aligner cannot learn from (every word in it co-occurs with every
        // other), so stuffing the counter would teach noise.
        self.add_exemplar(&hebrew, corrected);
        if was_translation {
            self.observe(&hebrew, corrected);
        }
        self.learn_mishearings(&produced, corrected);
        self.note_vocab(corrected);
        true
    }

    /// Apply whatever the night pass proposed, once, at startup.
    ///
    /// THE AUTHORITY SPLIT IS THE WHOLE DESIGN. Orel's own correction is ground truth: it locks a
    /// mishearing, feeds the aligner, and can force a rendering forever. A grader's opinion - even
    /// a strong model's, even when it is right - is EVIDENCE, and it enters at the weakest useful
    /// level:
    ///   - exemplars are added, because an exemplar is retrieved only for similar input and the
    ///     model is free to disregard it;
    ///   - mishearings are added UNLOCKED, so they are offered to the model as "maybe" and never
    ///     silently rewrite his words (`hint_block` in translate.rs draws that line);
    ///   - the aligner is NOT fed, because promotion produces a FORCED rendering and nothing
    ///     machine-generated has earned that;
    ///   - `corrected` on the log entry is NOT set, because that field means "Orel said so" and
    ///     overwriting it would destroy the only clean supervision signal the app has.
    ///
    /// Idempotent by archiving the file it consumed: a second startup finds nothing to do.
    pub fn ingest_proposals(&self) -> Option<String> {
        let path = self.dir.join("night-proposals.json");
        let raw = std::fs::read_to_string(&path).ok()?;
        let doc: Proposals = match serde_json::from_str(&raw) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[store] night-proposals.json unreadable ({e}); ignoring");
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                return None;
            }
        };

        for x in &doc.exemplars {
            if !x.hebrew.trim().is_empty() && !x.english.trim().is_empty() {
                self.add_exemplar(&x.hebrew, &x.english);
            }
        }

        if let Ok(mut dict) = self.dict.lock() {
            for m in &doc.mishearings {
                if m.heard.chars().count() < crate::phonetics::MIN_LEN || m.heard == m.meant {
                    continue;
                }
                // A rule Orel confirmed outranks anything proposed here - never downgrade it.
                if let Some(existing) = dict.mishearings.iter_mut().find(|e| e.heard == m.heard) {
                    if !existing.locked {
                        existing.meant = m.meant.clone();
                        existing.hits += 1;
                        existing.last_at = now_ms();
                    }
                } else {
                    dict.mishearings.push(Mishearing {
                        heard: m.heard.clone(),
                        meant: m.meant.clone(),
                        hits: 1,
                        locked: false,
                        last_at: now_ms(),
                    });
                }
            }
            write_json(&self.dir.join("dictionary.json"), &*dict);
        }

        let stamp = now_ms();
        let _ = std::fs::rename(&path, self.dir.join(format!("night-proposals-{stamp}.applied.json")));
        Some(doc.summary)
    }

    /// The one-line verdict from the last night pass, for the dashboard. Read from disk each time
    /// rather than cached: the pass runs in a separate process while the app is up, so a value
    /// held in memory would be stale exactly when it is interesting.
    pub fn night_summary(&self) -> String {
        std::fs::read_to_string(self.dir.join("night-pass-state.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("summary").and_then(|s| s.as_str()).map(str::to_string))
            .unwrap_or_default()
    }

    // ---- the automated dictionary ----

    /// Count one (Hebrew, English) pair. Every Hebrew content word is credited against every
    /// English content word in the same utterance; over many short utterances the true pairing
    /// outvotes the coincidences (a Dice-style co-occurrence aligner, no model involved).
    pub fn observe(&self, hebrew: &str, english: &str) {
        let he: Vec<String> = content_tokens(hebrew);
        let en: Vec<String> = content_tokens(english);
        if he.is_empty() || en.is_empty() || he.len() > 40 {
            return; // long utterances make co-occurrence meaningless; skip rather than pollute.
        }
        let Ok(mut dict) = self.dict.lock() else {
            return;
        };
        // Count each token ONCE per utterance: a word repeated in one sentence is one piece of
        // evidence about its translation, not three.
        let he_set = dedupe(&he);
        let en_set = dedupe(&en);
        for h in &he_set {
            *dict.he_seen.entry(h.clone()).or_insert(0) += 1;
            let row = dict.align.entry(h.clone()).or_default();
            for e in &en_set {
                *row.entry(e.clone()).or_insert(0) += 1;
            }
        }
        for e in &en_set {
            *dict.en_seen.entry(e.clone()).or_insert(0) += 1;
        }
        prune_align(&mut dict);
        promote(&mut dict);
        write_json(&self.dir.join("dictionary.json"), &*dict);
    }

    fn add_exemplar(&self, hebrew: &str, english: &str) {
        if let Ok(mut dict) = self.dict.lock() {
            dict.exemplars.retain(|x| x.hebrew != hebrew);
            dict.exemplars.push(Exemplar {
                hebrew: hebrew.to_string(),
                english: english.to_string(),
                at: now_ms(),
            });
            let overflow = dict.exemplars.len().saturating_sub(200);
            if overflow > 0 {
                dict.exemplars.drain(0..overflow);
            }
            write_json(&self.dir.join("dictionary.json"), &*dict);
        }
    }

    // ---- the mishearing layer ----

    /// Record what this utterance's FINAL text was made of. Vocabulary is built from output
    /// Orel accepted (or wrote himself), never from raw ASR - otherwise the first mishearing
    /// enters the vocabulary and starts attracting correct words toward itself.
    pub fn note_vocab(&self, text: &str) {
        if let Ok(mut dict) = self.dict.lock() {
            for w in content_tokens(text) {
                if w.chars().count() >= crate::phonetics::MIN_LEN {
                    *dict.vocab.entry(w).or_insert(0) += 1;
                }
            }
            write_json(&self.dir.join("dictionary.json"), &*dict);
        }
    }

    /// Words this transcript contains that are not known vocabulary but sound like something
    /// that is. Offered as suggestions, never applied - see `Mishearing::locked`.
    pub fn suspects(&self, text: &str) -> Vec<Suspect> {
        let Ok(dict) = self.dict.lock() else {
            return Vec::new();
        };
        let known = known_words(&dict);
        let mut out: Vec<Suspect> = Vec::new();
        for w in content_tokens(text) {
            if w.chars().count() < crate::phonetics::MIN_LEN || known.contains(&w) {
                continue;
            }
            let best = known
                .iter()
                .map(|k| (k.clone(), crate::phonetics::similarity(&w, k)))
                .filter(|(_, s)| *s >= SUSPECT_MIN_SCORE)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((meant, score)) = best {
                if meant != w {
                    out.push(Suspect { heard: w, meant, score });
                }
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(6);
        out
    }

    /// Apply the CONFIRMED rules. These came from Orel fixing the same word himself, so they
    /// are applied silently and word-boundary-exact - never as a substring replace, which
    /// would corrupt every word containing the pattern.
    pub fn apply_known(&self, text: &str) -> (String, usize) {
        let Ok(dict) = self.dict.lock() else {
            return (text.to_string(), 0);
        };
        let rules: Vec<&Mishearing> = dict.mishearings.iter().filter(|m| m.locked).collect();
        if rules.is_empty() {
            return (text.to_string(), 0);
        }
        let mut applied = 0usize;
        let out: String = split_keep(text)
            .into_iter()
            .map(|piece| {
                let bare = piece.trim_matches(|c: char| !c.is_alphanumeric());
                if bare.is_empty() {
                    return piece.to_string();
                }
                match rules.iter().find(|m| m.heard.eq_ignore_ascii_case(bare)) {
                    Some(rule) => {
                        applied += 1;
                        piece.replacen(bare, &rule.meant, 1)
                    }
                    None => piece.to_string(),
                }
            })
            .collect();
        (out, applied)
    }

    pub fn mishearings(&self) -> Vec<Mishearing> {
        let mut v = self
            .dict
            .lock()
            .map(|d| d.mishearings.clone())
            .unwrap_or_default();
        v.sort_by(|a, b| b.locked.cmp(&a.locked).then(b.hits.cmp(&a.hits)));
        v
    }

    pub fn forget_mishearing(&self, heard: &str) {
        if let Ok(mut dict) = self.dict.lock() {
            dict.mishearings.retain(|m| m.heard != heard);
            write_json(&self.dir.join("dictionary.json"), &*dict);
        }
    }

    /// Diff what the model produced against what Orel actually meant, and keep only the
    /// substitutions that SOUND alike. That filter is the whole design: a word swapped for a
    /// similar-sounding one is a mishearing and generalises to every future utterance; a word
    /// swapped for an unrelated one is Orel rephrasing, and generalising from it would corrupt
    /// later transcripts. Rephrasings are still captured - as exemplars, by `correct`.
    fn learn_mishearings(&self, produced: &str, corrected: &str) -> usize {
        let a = content_tokens(produced);
        let b = content_tokens(corrected);
        let pairs = align_substitutions(&a, &b);
        let mut learned = 0usize;
        if let Ok(mut dict) = self.dict.lock() {
            for (heard, meant) in pairs {
                if heard.chars().count() < crate::phonetics::MIN_LEN {
                    continue;
                }
                if crate::phonetics::similarity(&heard, &meant) < LEARN_MIN_SCORE {
                    continue; // a rephrase, not a mishearing
                }
                learned += 1;
                match dict.mishearings.iter_mut().find(|m| m.heard == heard) {
                    Some(m) => {
                        m.meant = meant;
                        m.hits += 1;
                        m.locked = true;
                        m.last_at = now_ms();
                    }
                    None => dict.mishearings.push(Mishearing {
                        heard,
                        meant,
                        hits: 1,
                        locked: true,
                        last_at: now_ms(),
                    }),
                }
            }
            if learned > 0 {
                write_json(&self.dir.join("dictionary.json"), &*dict);
            }
        }
        learned
    }

    pub fn glossary(&self) -> Vec<Term> {
        let mut terms = self
            .dict
            .lock()
            .map(|d| d.terms.clone())
            .unwrap_or_default();
        terms.sort_by(|a, b| b.hits.cmp(&a.hits));
        terms
    }

    /// Manual add/edit from the Settings tab. Anything Orel types is locked by definition.
    pub fn set_term(&self, he: &str, en: &str) {
        if let Ok(mut dict) = self.dict.lock() {
            match dict.terms.iter_mut().find(|t| t.he == he) {
                Some(t) => {
                    t.en = en.to_string();
                    t.locked = true;
                    t.last_at = now_ms();
                }
                None => dict.terms.push(Term {
                    he: he.to_string(),
                    en: en.to_string(),
                    hits: MIN_HITS,
                    locked: true,
                    last_at: now_ms(),
                }),
            }
            write_json(&self.dir.join("dictionary.json"), &*dict);
        }
    }

    pub fn forget_term(&self, he: &str) {
        if let Ok(mut dict) = self.dict.lock() {
            dict.terms.retain(|t| t.he != he);
            dict.align.remove(he);
            write_json(&self.dir.join("dictionary.json"), &*dict);
        }
    }

    /// The hints for one specific input: only terms whose Hebrew actually occurs in it, plus
    /// the closest past corrections. Scoped this way the prompt stays small and on-topic.
    pub fn hints_for(&self, hebrew: &str) -> Hints {
        let Ok(dict) = self.dict.lock() else {
            return Hints::default();
        };
        let tokens = content_tokens(hebrew);
        let mut terms: Vec<Term> = dict
            .terms
            .iter()
            .filter(|t| tokens.iter().any(|tok| tok == &t.he))
            .cloned()
            .collect();
        terms.sort_by(|a, b| b.locked.cmp(&a.locked).then(b.hits.cmp(&a.hits)));
        terms.truncate(MAX_TERM_HINTS);

        let mut scored: Vec<(f32, &Exemplar)> = dict
            .exemplars
            .iter()
            .map(|x| (overlap(&tokens, &content_tokens(&x.hebrew)), x))
            .filter(|(s, _)| *s >= MIN_OVERLAP)
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let exemplars = scored
            .into_iter()
            .take(MAX_EXEMPLARS)
            .map(|(_, x)| x.clone())
            .collect();
        drop(dict);

        Hints {
            terms,
            exemplars,
            suspects: self.suspects(hebrew),
        }
    }
}

/// Promote every aligned pair that now clears both bars. Locked terms are never touched.

/// A promoted pair must be TERMINOLOGY, never grammar. Measured 2026-08-06: the aligner had
/// promoted 67 pairs and 64 of them were ordinary Hebrew - `רוצה -> want`, `צריך -> need`,
/// `למה -> why` - which the translator already knows perfectly well, plus outright wrong ones
/// (`שזה -> their`, `בעברית -> write`, `עושה -> doesn't`). One of them, `לעשות -> system`,
/// is causally traceable to a corrupted paste: "we can system... use the instructional buttons".
/// Orel's own correction named the shape of the bug - לעשות is "to do", a VERB, and "system" is
/// מערכת, a NOUN. The aligner promoted a verb as if it were a term.
///
/// Two independent filters, because each one alone lets the other's cases through:
const MAX_TERM_SHARE: f32 = 0.15;

/// A token appearing in a large share of everything the speaker says is part of how he TALKS,
/// not part of what he talks about. Terminology is comparatively rare and consistent: `טרמינל`
/// appeared in 3 utterances, `רוצה` in 30.
///
/// The denominator is the MOST FREQUENT token's count, not the utterance count - the aligner
/// table never stored how many utterances it has seen, and inventing that number would be worse
/// than normalising against the busiest word, which tracks it closely enough to separate 3 from 30.
fn is_common_speech(he: &str, dict: &Dictionary, total: f32) -> bool {
    if total < 20.0 {
        return false; // too little history for the frequency signal to mean anything yet
    }
    dict.he_seen.get(he).copied().unwrap_or(0) as f32 / total > MAX_TERM_SHARE
}

/// Hebrew marks its verbs morphologically at the FRONT of the word, which is exactly what makes
/// this checkable without a tagger: an infinitive opens with ל, and the prefix conjugation opens
/// with א/ת/י/נ. That catches `לעשות`, `לראות`, `להמשיך`, `להשתמש`, `תעשה`, `תגיד`, `תפתח` -
/// every verb in the promoted set - while leaving borrowed nouns alone, because a transliterated
/// term does not carry Hebrew verbal morphology.
///
/// EXCEPTION, and it is the reason this is not a blunt prefix test: real terms DO begin with
/// those letters (`טרמינל` does not, but `למבדה`/lambda does, and ל is also the preposition "to"
/// glued onto a borrowed noun). So a token is spared when the lexicon knows it as a term.
/// SUBORDINATING CLITICS, added 2026-08-08 on measured evidence. The gate above held the
/// dictionary at 7 terms for a day and then it re-grew to 33, with `שנוכל -> install` among them.
/// `שנוכל` is "so that we can" - a conjugated verb wearing a ש prefix, which the bare
/// first-letter test cannot see because it only ever looks at position 0. Hebrew glues ש (that),
/// כש (when) and ו (and) onto the front of an already-conjugated verb, so the verbal marker moves
/// one or two characters right. Peel the clitic, then apply the same morphology test underneath.
fn looks_verbal(he: &str) -> bool {
    let ch: Vec<char> = he.chars().collect();
    if ch.len() >= 5 && ch[0] == 'כ' && ch[1] == 'ש' && is_verb_stem(&ch[2..]) {
        return true;
    }
    if ch.len() >= 4 && (ch[0] == 'ש' || ch[0] == 'ו') && is_verb_stem(&ch[1..]) {
        return true;
    }
    is_verb_stem(&ch)
}

/// The morphology test proper, on a clitic-free stem. Hebrew marks its verbs at the FRONT: an
/// infinitive opens with ל, and the prefix conjugation opens with א/ת/י/נ.
fn is_verb_stem(ch: &[char]) -> bool {
    if ch.len() < 4 {
        return false;
    }
    match ch[0] {
        'ל' => ch.len() >= 5,           // infinitive: לעשות, להשתמש
        'ת' | 'י' | 'נ' | 'א' => ch.len() >= 4 && ch.len() <= 6, // תעשה, תגיד, נראה
        _ => false,
    }
}

/// The third filter, and the one the other two structurally cannot do: some grammar is invisible
/// on the Hebrew side. `בעברית -> write`, `שוב -> again`, `ממש -> really`, `עשית -> did` all
/// survived the frequency and morphology gates, and none of them is a term. What gives them away
/// is the ENGLISH side: terminology renders to a domain noun, grammar renders to a closed-class
/// word or a bare common verb.
///
/// This list is deliberately small and boring. It is not a dictionary of English - it is the
/// closed class plus the handful of verbs that showed up in the measured failure set. Anything
/// not on it is allowed through, because the cost of a missing term is one weaker prompt and the
/// cost of a wrong forced rendering is a corrupted paste.
const EN_GRAMMAR: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "so", "then", "than", "as", "at", "by", "for",
    "from", "in", "into", "of", "on", "onto", "to", "with", "within", "without", "about",
    "again", "also", "any", "all", "already", "always", "another", "because", "before", "after",
    "between", "both", "even", "ever", "every", "here", "there", "how", "what", "when", "where",
    "which", "who", "why", "instead", "itself", "just", "like", "maybe", "more", "most", "much",
    "never", "now", "only", "other", "others", "our", "out", "outside", "over", "really", "same",
    "second", "side", "since", "some", "something", "still", "such", "that", "their", "them",
    "themselves", "these", "they", "thing", "things", "this", "those", "through", "too", "under",
    "up", "very", "we", "well", "what", "while", "you", "your", "terms", "example", "addition",
    "be", "been", "being", "can", "come", "continue", "did", "do", "does", "done", "feel", "get",
    "give", "go", "had", "has", "have", "is", "keep", "know", "let", "look", "looks", "make",
    "made", "need", "open", "put", "said", "say", "see", "seen", "should", "take", "tell", "think",
    "use", "used", "want", "was", "were", "will", "would", "write", "wrote", "okay", "great",
    // Added 2026-08-08 from the second re-growth (7 -> 33 terms in two days). Every one of these
    // was promoted as "terminology" and every one is ordinary vocabulary the translator already
    // renders correctly unaided - so the hint bought nothing and cost prompt tokens on the way in.
    "leave", "left", "first", "new", "less", "together", "meaning", "according", "sent", "send",
    "start", "stop", "close", "finish", "change", "move", "add", "remove", "try", "wait", "check",
    "work", "works", "worked", "sure", "better", "best", "big", "small", "fast", "slow",
];

/// An -ly adverb is never terminology. This is the general form of half the stoplist above and it
/// costs nothing: `automatically`, `realistically`, `basically` are things the model already knows,
/// and forcing a rendering for them can only make output worse.
fn is_adverb(en: &str) -> bool {
    let w = en.trim().to_lowercase();
    w.len() > 4 && w.ends_with("ly") && !w.contains(' ')
}

fn is_english_grammar(en: &str) -> bool {
    let w = en.trim().to_lowercase();
    (!w.contains(' ') && EN_GRAMMAR.contains(&w.as_str())) || is_adverb(&w)
}

fn promote(dict: &mut Dictionary) {
    let candidates: Vec<(String, String, u32)> = dict
        .align
        .iter()
        .filter_map(|(he, row)| {
            let he_n = *dict.he_seen.get(he)? as f32;
            let (en, hits, dice, margin) = best_pair(row, &dict.en_seen, he_n)?;
            // A word that renders as itself teaches the model nothing.
            let total = dict.he_seen.values().copied().max().unwrap_or(0) as f32;
            (hits >= MIN_HITS
                && dice >= MIN_DICE
                && margin >= MIN_MARGIN
                && &en != he
                && !is_common_speech(he, dict, total)
                && !looks_verbal(he)
                && !is_english_grammar(&en))
                .then(|| (he.clone(), en, hits))
        })
        .collect();

    for (he, en, hits) in candidates {
        match dict.terms.iter_mut().find(|t| t.he == he) {
            Some(t) if t.locked => {}
            Some(t) => {
                t.en = en;
                t.hits = hits;
                t.last_at = now_ms();
            }
            None => dict.terms.push(Term {
                he,
                en,
                hits,
                locked: false,
                last_at: now_ms(),
            }),
        }
    }
}

/// The best English rendering for one Hebrew token, by Dice coefficient:
/// `2 * co-occurrences / (times the Hebrew appeared + times the English appeared)`.
///
/// Returns the MARGIN over the runner-up as well, and that margin is what makes the aligner
/// safe. Inside one sentence every Hebrew word co-occurs with every English word exactly as
/// often, so they all score identically - repeating that sentence a hundred times raises the
/// scores but never separates them. Requiring the winner to beat the second place by a real
/// distance is what turns "seen a lot" into "actually distinguished".
///
/// Ties break toward the RARER English word (more information), then lexicographically, because
/// HashMap iteration order is not stable across runs and a dictionary must be reproducible.
fn best_pair(
    row: &HashMap<String, u32>,
    en_seen: &HashMap<String, u32>,
    he_n: f32,
) -> Option<(String, u32, f32, f32)> {
    let mut scored: Vec<(String, u32, f32, f32)> = row
        .iter()
        .map(|(en, &c)| {
            let en_n = *en_seen.get(en).unwrap_or(&c) as f32;
            let dice = (2.0 * c as f32) / (he_n + en_n).max(1.0);
            (en.clone(), c, dice, en_n)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.2.total_cmp(&a.2)
            .then(a.3.total_cmp(&b.3))
            .then(a.0.cmp(&b.0))
    });
    let best = scored.first()?;
    let margin = best.2 - scored.get(1).map_or(0.0, |s| s.2);
    Some((best.0.clone(), best.1, best.2, margin))
}

/// Keep the tables bounded by dropping the coldest Hebrew tokens once they overflow.
fn prune_align(dict: &mut Dictionary) {
    if dict.align.len() <= ALIGN_CAP {
        return;
    }
    let mut by_weight: Vec<(String, u32)> = dict
        .he_seen
        .iter()
        .map(|(k, n)| (k.clone(), *n))
        .collect();
    by_weight.sort_by(|a, b| a.1.cmp(&b.1));
    for (key, _) in by_weight.into_iter().take(dict.align.len() - ALIGN_CAP) {
        dict.align.remove(&key);
        dict.he_seen.remove(&key);
    }
}

/// Age the learned tables on load. Without this an early wrong pairing outranks current usage
/// forever, because nothing ever removes weight - only adds it. Locked entries are exempt:
/// those are Orel's own corrections and are not guesses that can go stale.
fn decay(dict: &mut Dictionary) {
    let now = now_ms();
    let stale = |last: u64| last > 0 && now.saturating_sub(last) > DECAY_AFTER_MS;
    for t in dict.terms.iter_mut() {
        if !t.locked && stale(t.last_at) {
            t.hits /= 2;
        }
    }
    dict.terms.retain(|t| t.locked || t.hits > 0);
    for m in dict.mishearings.iter_mut() {
        if !m.locked && stale(m.last_at) {
            m.hits /= 2;
        }
    }
    dict.mishearings.retain(|m| m.locked || m.hits > 0);
}

/// Everything the speaker is known to say: the seed list, learned glossary renderings, and
/// every word that survived into an accepted output.
fn known_words(dict: &Dictionary) -> Vec<String> {
    let mut set: Vec<String> = SEED_VOCAB.iter().map(|s| s.to_string()).collect();
    for t in &dict.terms {
        for w in content_tokens(&t.en) {
            if !set.contains(&w) {
                set.push(w);
            }
        }
    }
    for (w, n) in &dict.vocab {
        // One sighting is not vocabulary - it may itself have been the mishearing.
        if *n >= MISHEARING_MIN_HITS && !set.contains(w) {
            set.push(w.clone());
        }
    }
    set
}

/// Token-level substitutions between two sequences, via an LCS alignment. Runs of unmatched
/// tokens on both sides are paired positionally; a 1-to-many run also yields the joined form,
/// so "comic" -> "commit and" is recoverable and not just dropped as a length mismatch.
fn align_substitutions(a: &[String], b: &[String]) -> Vec<(String, String)> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    // Emit a proper edit script first. An earlier version tried to collect the diverging run
    // on each side with two independent loops, which could advance only one of them and hand
    // back a run of deletions paired against nothing - measured, it produced zero
    // substitutions for the textbook case. Deriving deletions and insertions in one pass and
    // grouping them afterwards removes that whole class of bug.
    enum Op {
        Keep,
        Del(String),
        Ins(String),
    }
    let mut ops: Vec<Op> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(Op::Keep);
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            ops.push(Op::Del(a[i].clone()));
            i += 1;
        } else {
            ops.push(Op::Ins(b[j].clone()));
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Del(a[i].clone()));
        i += 1;
    }
    while j < m {
        ops.push(Op::Ins(b[j].clone()));
        j += 1;
    }

    // A substitution is a maximal run of deletions and insertions with no match between them.
    let mut out = Vec::new();
    let mut dels: Vec<String> = Vec::new();
    let mut ins: Vec<String> = Vec::new();
    let mut flush = |dels: &mut Vec<String>, ins: &mut Vec<String>, out: &mut Vec<(String, String)>| {
        if !dels.is_empty() && !ins.is_empty() {
            if dels.len() == 1 {
                // One word became several: "comic" -> "commit and". Keep the joined form so a
                // collapsed phrase is recoverable, not discarded as a length mismatch.
                out.push((dels[0].clone(), ins.join(" ")));
            } else {
                for (x, y) in dels.iter().zip(ins.iter()) {
                    out.push((x.clone(), y.clone()));
                }
            }
        }
        dels.clear();
        ins.clear();
    };
    for op in ops {
        match op {
            Op::Keep => flush(&mut dels, &mut ins, &mut out),
            Op::Del(w) => dels.push(w),
            Op::Ins(w) => ins.push(w),
        }
    }
    flush(&mut dels, &mut ins, &mut out);
    out
}

/// Split on whitespace but keep the pieces intact, so punctuation attached to a word survives
/// a rewrite ("comic," -> "commit,").
fn split_keep(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut last = 0usize;
    for (idx, c) in text.char_indices() {
        if c.is_whitespace() {
            if idx > last {
                out.push(&text[last..idx]);
            }
            out.push(&text[idx..idx + c.len_utf8()]);
            last = idx + c.len_utf8();
        }
    }
    if last < text.len() {
        out.push(&text[last..]);
    }
    out
}

/// One vote per token per utterance.
fn dedupe(tokens: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tokens {
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    out
}

/// Words worth aligning: 3+ characters, not a function word. Hebrew has no casing, so the
/// only normalisation needed is lowercasing the Latin side and stripping punctuation.
fn content_tokens(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "את", "של", "על", "אני", "אתה", "זה", "לא", "כן", "יש", "אין", "עם", "אבל", "גם", "רק",
        "כמו", "מה", "איך", "כדי", "היא", "הוא", "the", "and", "for", "you", "that", "this",
        "with", "are", "was", "not", "its", "from", "have", "there", "then", "into",
    ];
    text.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '_' && c != '.')
        .map(|w| w.trim_matches('.').to_lowercase())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.as_str()))
        .collect()
}

/// How long after a paste a repeat still counts as a repeat OF that paste. Three minutes: long
/// enough to read the bad English, swear, and say it again; short enough that the next unrelated
/// thought on the same subject has moved on.
const REDICTATION_WINDOW_MS: u64 = 180_000;

/// Token overlap above which two utterances are "the same thing again".
///
/// MEASURED, and the measurement demoted this whole feature. Replayed over his own 274-entry log
/// on 2026-08-08: of 200 eligible consecutive pairs, exactly ONE fires - and it fires identically
/// at every threshold from 0.3 to 0.7, because the single real repeat scores 1.0 and nothing else
/// scores above 0.3. So this speaker does not re-dictate when the output is wrong; he takes the
/// bad paste and moves on.
///
/// It stays because it costs nothing and a free label is still a free label, but it is a BONUS
/// signal, not a source. The mechanism that actually improves the translator without his labour
/// is the night pass, which uses a strong model as its oracle and needs no human verdict at all.
/// 0.5 is kept because the choice is insensitive: every threshold in that range gives one fire.
const REDICTATION_MIN_OVERLAP: f32 = 0.5;

/// Did the speaker just say the same thing again? The signal is deliberately conservative in both
/// directions - a missed repeat costs one unused label, a false repeat teaches the system that a
/// GOOD translation was rejected, which is strictly worse.
fn is_redictation(prev: &LogEntry, next: &LogEntry) -> bool {
    if next.at.saturating_sub(prev.at) > REDICTATION_WINDOW_MS {
        return false;
    }
    // A repeat is a repeat of the same KIND of pass; a polish following a translate is a mode
    // change, not a complaint.
    if prev.mode != next.mode {
        return false;
    }
    let a = content_tokens(&prev.hebrew);
    let b = content_tokens(&next.hebrew);
    // Very short utterances share tokens by accident ("okay", "yes, do it"), so the overlap
    // measure carries no information there.
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    overlap(&a, &b) >= REDICTATION_MIN_OVERLAP
}

/// Jaccard overlap between two token sets - cheap fuzzy match for translation-memory recall.
fn overlap(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|t| b.contains(t)).count() as f32;
    let union = (a.len() + b.len()) as f32 - shared;
    if union <= 0.0 {
        0.0
    } else {
        shared / union
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&raw) {
        Ok(v) => Some(v),
        Err(e) => {
            // A corrupt store must never take the app down: rename it aside and start clean.
            eprintln!("[store] {} unreadable ({e}); starting fresh", path.display());
            let _ = fs::rename(path, path.with_extension("json.bad"));
            None
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    let Ok(text) = serde_json::to_string(value) else {
        return;
    };
    // Write-then-rename: a crash mid-write leaves the previous good file, never a half one.
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let dir = std::env::temp_dir().join(format!("orellius-stt-test-{}", now_ms()));
        Store::load(dir)
    }

    /// The authority split, tested where it actually matters: a night-pass proposal must be able
    /// to add knowledge and must NOT be able to overrule Orel. A grader that could silently
    /// downgrade a locked rule would quietly undo the only supervision this app has.
    #[test]
    fn night_proposals_add_evidence_but_never_outrank_him() {
        let s = store();
        // A rule he confirmed by correcting it.
        s.learn_mishearings("comic push the branch", "commit push the branch");
        let locked_before = s.mishearings().into_iter().find(|m| m.heard == "comic");
        assert!(locked_before.as_ref().is_some_and(|m| m.locked), "his correction should lock");

        std::fs::write(
            s.dir.join("night-proposals.json"),
            r#"{"summary":"test pass","exemplars":[{"hebrew":"תעשה קומיט","english":"Make a commit."}],
                "mishearings":[{"heard":"comic","meant":"comedy"},{"heard":"flutz","meant":"plot"}]}"#,
        )
        .unwrap();

        let summary = s.ingest_proposals().expect("proposals should be ingested");
        assert_eq!(summary, "test pass");

        let after = s.mishearings();
        let comic = after.iter().find(|m| m.heard == "comic").expect("locked rule survives");
        assert_eq!(comic.meant, "commit", "a proposal must not overwrite his locked rule");
        let flutz = after.iter().find(|m| m.heard == "flutz").expect("new rule added");
        assert!(!flutz.locked, "machine-proposed rules are offered, never applied silently");

        // Consumed exactly once: a second startup must not re-apply the same file.
        assert!(s.ingest_proposals().is_none(), "the proposals file should be archived");
    }

    /// The one real repeat in his log, and the neighbouring pair that must NOT fire. Both are
    /// lifted from live data, because a similarity threshold defended by an invented percentage is
    /// how the previous version of this comment was wrong.
    #[test]
    fn a_repeat_is_flagged_and_a_follow_up_thought_is_not() {
        let e = |at: u64, he: &str| LogEntry {
            at,
            hebrew: he.to_string(),
            mode: "translate".to_string(),
            ..Default::default()
        };
        let first = e(1_000, "תחשב את זה בזמן שלך, לא בזמן של בני אדם.");
        let again = e(20_000, "תחשב את זה בזמן שלך, לא של בני אדם.");
        assert!(is_redictation(&first, &again), "the same sentence again must flag");

        // Same topic, different thought - the case a looser bar would wrongly punish.
        let a = e(1_000, "תוסיף אפקט לכסף שהם מרוויחים בתוך המשחק");
        let b = e(20_000, "בנוסף לזה אני רוצה שיהיה עוד currency מחוץ לסשן");
        assert!(!is_redictation(&a, &b), "a follow-up thought is not a complaint");

        // Outside the window, the same words are a new request, not a repeat.
        let late = e(1_000 + REDICTATION_WINDOW_MS + 1, "תחשב את זה בזמן שלך, לא של בני אדם.");
        assert!(!is_redictation(&first, &late), "past the window it is a new request");
    }

    /// The gate is only worth anything if it can be SEEN to reject the exact pairs that got past
    /// it. These six are lifted verbatim from the live dictionary on 2026-08-08, after it re-grew
    /// from 7 terms to 33 in two days. A gate proven only by "nothing bad appeared" is a gate
    /// nobody has watched fire (SCAR-004).
    #[test]
    fn the_gate_rejects_the_pairs_that_actually_got_through() {
        // Hebrew side: a conjugated verb hiding behind a subordinating clitic.
        assert!(looks_verbal("שנוכל"), "so-that-we-can is a verb wearing a ש");
        assert!(looks_verbal("שתעשה"));
        assert!(looks_verbal("כשנראה"));
        assert!(looks_verbal("ותגיד"));
        // English side: ordinary vocabulary the translator already knows unaided.
        for en in ["leave", "first", "left", "new", "less", "together", "meaning", "according"] {
            assert!(is_english_grammar(en), "{en} should be blocked as ordinary vocabulary");
        }
        assert!(is_english_grammar("automatically"), "-ly adverbs are never terminology");
        assert!(is_english_grammar("realistically"));
    }

    /// The other direction, and the one that matters more: real terminology must still pass. A
    /// filter that blocks everything is not a fix, it is a broken dictionary with a clean log.
    #[test]
    fn the_gate_still_admits_real_terminology() {
        for he in ["טרמינל", "קפיברה", "המוקאפ", "רובלוקס", "הכסף", "קומיט"] {
            assert!(!looks_verbal(he), "{he} is a noun, not a verb");
        }
        for en in ["terminal", "capybara", "mockup", "roblox", "commit", "repo", "plot", "money"] {
            assert!(!is_english_grammar(en), "{en} is terminology and must survive");
        }
        // A short word ending in -ly is a word, not an adverb: "fly", "only" is in the list anyway.
        assert!(!is_adverb("fly"));
    }

    /// The bar must bite in both directions: one sighting teaches nothing, and a term that
    /// recurs ACROSS DIFFERENT SENTENCES becomes a forced rendering.
    #[test]
    fn aligner_learns_a_term_that_recurs_across_contexts() {
        let s = store();
        s.observe("תפתח את הקומיט", "open the commit");
        assert!(
            s.hints_for("תפתח את הקומיט").terms.is_empty(),
            "one observation must not promote anything"
        );
        s.observe("תראה לי את הקומיט", "show me the commit");
        s.observe("הקומיט הזה שבור", "this commit is broken");

        let terms = s.hints_for("בוא נראה את הקומיט").terms;
        assert!(
            terms.iter().any(|t| t.he == "הקומיט" && t.en == "commit"),
            "term recurring in 3 contexts must promote, got {:?}",
            terms.iter().map(|t| (&t.he, &t.en)).collect::<Vec<_>>()
        );
        assert!(
            !terms.iter().any(|t| t.en == "open"),
            "a word seen beside it only once must not promote"
        );
    }

    /// The failure this design was corrected for, kept as a permanent guard: repeating ONE
    /// sentence is not evidence. Inside a single sentence every Hebrew word co-occurs with
    /// every English word equally, so nothing in it can be told apart - and a dictionary that
    /// "learns" from it would confidently force the wrong rendering forever.
    #[test]
    fn one_sentence_repeated_teaches_nothing() {
        let s = store();
        for _ in 0..10 {
            s.observe("תפתח את הקומיט", "open the commit");
        }
        let terms = s.hints_for("תפתח את הקומיט").terms;
        assert!(
            terms.is_empty(),
            "a single sentence cannot disambiguate, got {:?}",
            terms.iter().map(|t| (&t.he, &t.en)).collect::<Vec<_>>()
        );
    }

    /// Hints are scoped to the input. A term for words that are not in this sentence would
    /// be prompt noise, and noise is what makes an instruct model drift.
    #[test]
    fn hints_are_scoped_to_the_sentence() {
        let s = store();
        s.set_term("בראנץ", "branch");
        assert!(s.hints_for("תעשה בראנץ חדש").terms.len() == 1);
        assert!(s.hints_for("תריץ את הטסטים").terms.is_empty());
    }

    /// The live defect this whole layer exists for. "commit and push" came back as "comic
    /// push" on 2026-08-06; correcting it once must teach the app to fix it silently forever.
    #[test]
    fn a_correction_teaches_a_misheard_word() {
        let s = store();
        s.append_log(LogEntry {
            at: 111,
            hebrew: "תעשה comic push".into(),
            english: "do a comic push".into(),
            mode: "translate".into(),
            ..Default::default()
        });
        assert!(s.correct(111, "do a commit and push"));

        let (fixed, applied) = s.apply_known("another comic push please");
        assert_eq!(applied, 1, "the learned rule did not fire");
        assert!(fixed.contains("commit"), "got {fixed:?}");
        assert!(!fixed.contains("comic"), "got {fixed:?}");
    }

    /// The other direction, and the one that keeps this safe: when Orel REPHRASES rather than
    /// fixes a mishearing, the words do not sound alike and nothing may be learned. Without
    /// this filter every edit would become a permanent global find-and-replace.
    #[test]
    fn a_rephrase_teaches_nothing() {
        let s = store();
        s.append_log(LogEntry {
            at: 222,
            hebrew: "תבדוק".into(),
            english: "check the server logs".into(),
            mode: "translate".into(),
            ..Default::default()
        });
        assert!(s.correct(222, "check the database logs"));

        assert!(
            s.mishearings().is_empty(),
            "learned a rule from a rephrase: {:?}",
            s.mishearings().iter().map(|m| (&m.heard, &m.meant)).collect::<Vec<_>>()
        );
        let (text, applied) = s.apply_known("restart the server now");
        assert_eq!(applied, 0);
        assert!(text.contains("server"), "a rephrase must not rewrite later text");
    }

    /// Punctuation must survive a silent rewrite, and only whole words may match - a substring
    /// replace would corrupt every word that happens to contain the pattern.
    #[test]
    fn rewrites_respect_word_boundaries_and_punctuation() {
        let s = store();
        s.append_log(LogEntry {
            at: 333,
            hebrew: "x".into(),
            english: "run the tesst".into(),
            mode: "translate".into(),
            ..Default::default()
        });
        assert!(s.correct(333, "run the tests"));
        let (fixed, n) = s.apply_known("run the tesst, then deploy");
        assert_eq!(n, 1);
        assert!(fixed.contains("tests,"), "punctuation lost: {fixed:?}");
    }

    /// Orel's word outranks the counter: a locked term survives contradicting observations.
    #[test]
    fn a_locked_term_is_never_overwritten_by_the_aligner() {
        let s = store();
        s.set_term("ריפו", "repository");
        for _ in 0..MIN_HITS * 3 {
            s.observe("תפתח ריפו", "open repo");
        }
        let t = s.hints_for("תפתח ריפו").terms;
        assert_eq!(t[0].en, "repository", "locked term must win");
    }

    /// The live failure, from his own dictionary on 2026-08-06, and his own correction of it:
    /// לעשות is "to do" (a VERB) and "system" is מערכת (a NOUN). The aligner had promoted the
    /// verb as terminology, and the wrong rendering reached a real paste.
    #[test]
    fn verbs_are_never_promoted_as_terminology() {
        for verb in ["לעשות", "להשתמש", "לראות", "תעשה", "תגיד", "תפתח"] {
            assert!(looks_verbal(verb), "{verb} should read as verbal morphology");
        }
    }

    /// The other direction, which is the half that makes the gate a gate rather than a wall:
    /// borrowed terminology must survive it.
    #[test]
    fn borrowed_terms_survive_the_verb_filter() {
        for term in ["טרמינל", "קפיברה", "ריפו", "בראנץ", "קומיט"] {
            assert!(!looks_verbal(term), "{term} must not read as a verb");
        }
    }

    /// Frequency: a word used in a third of everything he says is grammar. One used three times
    /// in ninety is a term.
    #[test]
    fn common_speech_is_rejected_and_rare_terms_kept() {
        let mut dict = Dictionary::default();
        dict.he_seen.insert("רוצה".into(), 30);
        dict.he_seen.insert("טרמינל".into(), 3);
        assert!(is_common_speech("רוצה", &dict, 30.0));
        assert!(!is_common_speech("טרמינל", &dict, 30.0));
        // Under twenty observations the signal is noise, and the gate says so rather than guessing.
        assert!(!is_common_speech("רוצה", &dict, 12.0));
    }

    /// The cases that walked through both Hebrew-side filters and are still not terminology.
    #[test]
    fn english_grammar_renderings_are_rejected() {
        for en in ["write", "again", "really", "did", "things", "any", "terms", "instead"] {
            assert!(is_english_grammar(en), "{en} should be rejected as grammar");
        }
        for en in ["terminal", "capybara", "mockup", "commit", "repository", "endpoint"] {
            assert!(!is_english_grammar(en), "{en} is a term and must survive");
        }
    }
}

