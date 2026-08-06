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
    /// "translate" | "polish" | "raw"
    pub mode: String,
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

/// What translate.rs injects for one specific Hebrew input.
#[derive(Clone, Default, Serialize)]
pub struct Hints {
    pub terms: Vec<Term>,
    pub exemplars: Vec<Exemplar>,
}

impl Hints {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.exemplars.is_empty()
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
}

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
        Self {
            settings: Mutex::new(read_json(&dir.join("settings.json")).unwrap_or_default()),
            log: Mutex::new(read_json(&dir.join("log.json")).unwrap_or_default()),
            rejections: Mutex::new(read_json(&dir.join("rejections.json")).unwrap_or_default()),
            dict: Mutex::new(read_json(&dir.join("dictionary.json")).unwrap_or_default()),
            dir,
        }
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
        write_json(&self.dir.join("log.json"), &*log);
        drop(log);

        // The exemplar is what carries a correction's weight. It is counted ONCE, deliberately:
        // replaying one sentence N times is the degenerate input the aligner cannot learn from
        // (every word in it co-occurs with every other), so stuffing the counter would teach noise.
        self.add_exemplar(&hebrew, corrected);
        self.observe(&hebrew, corrected);
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

        Hints { terms, exemplars }
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
