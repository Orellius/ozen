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
fn promote(dict: &mut Dictionary) {
    let candidates: Vec<(String, String, u32)> = dict
        .align
        .iter()
        .filter_map(|(he, row)| {
            let he_n = *dict.he_seen.get(he)? as f32;
            let (en, hits, dice, margin) = best_pair(row, &dict.en_seen, he_n)?;
            // A word that renders as itself teaches the model nothing.
            (hits >= MIN_HITS && dice >= MIN_DICE && margin >= MIN_MARGIN && &en != he)
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
}

