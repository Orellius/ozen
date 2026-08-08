//! lexicon.rs: the researched Hebrew lexicon, compiled into the binary and looked up per clip.
//! Public surface: hints_for(hebrew) -> Vec<Term>, len() (for diagnostics).
//! Why this file (vs the learned dictionary in store.rs): the learned dictionary is what THIS
//!   speaker taught the app by correcting it, and it starts empty. This is what is true about
//!   Israeli spoken Hebrew before Orel says a word - slang whose literal translation is wrong,
//!   and English dev vocabulary the transcript writes in Hebrew letters. They are merged at the
//!   prompt, and the learned side wins on collision because his correction outranks a table.
//! NOT responsible for: learning, promotion, persistence. This table never changes at runtime.
//! Test strategy: assert a clitic-prefixed dev term resolves ("הקומיט" -> commit) and that a
//!   word merely CONTAINING a lexicon entry does not match.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::store::Term;

/// Compiled in rather than read from disk: the app is a signed bundle, and a data file sitting
/// next to it is one more thing that can go missing, go stale, or be edited into a prompt
/// injection. The research corpus lives in the repo at docs/hebrew/ and travels with the binary.
const RAW: &str = include_str!("../../docs/hebrew/lexicon.json");

/// Bound on how much of the table may enter one prompt. The whole lexicon is 304 entries; a
/// prompt carrying all of them would cost more context than the sentence it is helping and
/// would bury the instruction it is supposed to support.
const MAX_HINTS: usize = 12;

#[derive(Deserialize)]
struct Doc {
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    class: String,
    #[serde(default)]
    he: Option<String>,
    #[serde(default)]
    en: Vec<String>,
    #[serde(default)]
    latin: Option<String>,
    #[serde(default)]
    interpreter: Option<String>,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default)]
    forms: Option<Forms>,
}

#[derive(Deserialize, Default)]
struct Forms {
    #[serde(default)]
    prefixed_hebrew_script: Vec<String>,
    #[serde(default)]
    prefixed_plural: Vec<String>,
    #[serde(default)]
    plural_he: Option<String>,
    #[serde(default)]
    verbalised: Option<String>,
}

/// Surface form -> the rendering to suggest. Every inflected form the research generated is a
/// key, because Hebrew glues ה/ו/ב/ל/מ/ש/כ straight onto a borrowed word: "בקומיט" is ONE token,
/// and a table keyed only on the lemma sees nothing at all in real speech.
static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();

fn table() -> &'static HashMap<String, String> {
    TABLE.get_or_init(|| {
        let mut map: HashMap<String, String> = HashMap::new();
        let doc: Doc = match serde_json::from_str(RAW) {
            Ok(d) => d,
            Err(e) => {
                // A malformed table must never take dictation down with it: the pipeline runs
                // without lexicon hints, exactly as it did before this file existed.
                eprintln!("[lexicon] parse failed, running without it: {e}");
                return map;
            }
        };
        for e in doc.entries {
            // A dev term has ONE correct rendering (the Latin original it was borrowed from).
            // Slang has several, and the `interpreter` line is the researched one - what a human
            // interpreter says, rather than the literal gloss that turns "יאללה" into "O God".
            let rendering = match e.class.as_str() {
                "dev-transliteration" => e.latin.clone().or_else(|| e.en.first().cloned()),
                _ => e
                    .interpreter
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| e.en.first().cloned()),
            };
            let Some(rendering) = rendering else { continue };
            let rendering = flatten(&rendering);

            let mut keys: Vec<String> = Vec::new();
            if let Some(he) = &e.he {
                keys.push(he.clone());
            }
            keys.extend(e.variants.iter().cloned());
            if let Some(f) = &e.forms {
                keys.extend(f.prefixed_hebrew_script.iter().cloned());
                keys.extend(f.prefixed_plural.iter().cloned());
                keys.extend(f.plural_he.iter().cloned());
                keys.extend(f.verbalised.iter().cloned());
            }
            for k in keys {
                let k = k.trim().to_string();
                // One-character keys would match half the language. The morphology entries carry
                // patterns rather than surface forms; they are documentation, not lookup keys,
                // and they have no `he` field, so they never reach here.
                if k.chars().count() < 2 {
                    continue;
                }
                map.entry(k).or_insert_with(|| rendering.clone());
            }
        }
        map
    })
}

/// One line, bounded - the same discipline the learned hints follow, and for the same reason: a
/// multi-line value breaks the reference-table framing that keeps this data from reading as
/// instructions to the model.
fn flatten(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect()
}

/// How many surface forms the table holds. Diagnostics only.
pub fn len() -> usize {
    table().len()
}

/// The lexicon entries this ONE utterance actually contains, as prompt terms.
///
/// Matching is on WHOLE tokens. Substring matching would fire constantly - short Hebrew roots sit
/// inside dozens of longer words - and a wrong forced rendering is permanent and invisible, which
/// is the same reason the aligner's promotion bars are set as high as they are.
pub fn hints_for(hebrew: &str) -> Vec<Term> {
    let t = table();
    if t.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Term> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for raw in hebrew.split_whitespace() {
        // Punctuation is not part of the word, but the hyphen and the maqaf ARE part of forms
        // like "ה-commit", so they survive the trim.
        let tok = raw.trim_matches(|c: char| {
            !(c.is_alphanumeric() || c == '-' || c == '\u{05BE}' || c == '\'' || c == '"')
        });
        if tok.chars().count() < 2 || seen.iter().any(|s| s == tok) {
            continue;
        }
        if let Some(en) = t.get(tok) {
            seen.push(tok.to_string());
            out.push(Term {
                he: tok.to_string(),
                en: en.clone(),
                hits: 0,
                locked: false,
                last_at: 0,
            });
            if out.len() >= MAX_HINTS {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table must actually load. A silent parse failure leaves every lookup empty and looks
    /// exactly like "the lexicon did not help much", which is unfalsifiable from the outside.
    #[test]
    fn table_loads_and_is_large() {
        assert!(len() > 500, "only {} surface forms", len());
    }

    /// The case the forms expansion exists for: Hebrew glues its prepositions onto the borrowed
    /// word, so the lemma alone never matches real speech.
    #[test]
    fn clitic_prefixed_dev_terms_resolve() {
        let h = hints_for("תעשה את הקומיט ואז תדחוף");
        let pairs: Vec<String> = h.iter().map(|t| format!("{}={}", t.he, t.en)).collect();
        assert!(h.iter().any(|t| t.en == "commit"), "got {pairs:?}");
    }

    /// Whole tokens only. A longer word that merely contains a lexicon key must not match -
    /// substring matching is how a lexicon starts rewriting words nobody asked it to touch.
    #[test]
    fn matching_is_whole_token() {
        assert!(hints_for("קומיטיםxxבלתיקיימים").is_empty());
    }

    /// The measured mishearings from 2026-08-08, each one a sentence the app actually got wrong.
    /// These are pinned by name because a proper noun is precisely what a translator cannot infer:
    /// there is nothing in `רוזן` to tell a model it means a product called Ozen.
    #[test]
    fn the_names_it_used_to_mangle_now_resolve() {
        for (sentence, want) in [
            ("אני משתמש ברוזן כל היום", "Ozen"),        // shipped as "Rosen"
            ("תעשה adjustment לפיירבול", "fireball"),   // shipped as "firewall"
            ("עשיתי רלוד לאקסטינשן", "reload"),         // shipped as "reroll"
            ("איך לעשות את האפרוץ הזה", "fanout"),      // shipped as "break-in"
            ("תפתח את הקלודקוד", "Claude Code"),
            ("תשאל את קלוד", "Claude"),
        ] {
            let h = hints_for(sentence);
            let got: Vec<String> = h.iter().map(|t| format!("{}={}", t.he, t.en)).collect();
            assert!(
                h.iter().any(|t| t.en == want),
                "{sentence:?} should hint {want:?}, got {got:?}"
            );
        }
    }

    /// Slang carries the interpreter's rendering, never the literal one.
    #[test]
    fn slang_uses_the_interpreter_line() {
        let h = hints_for("יאללה תתחיל");
        assert!(!h.is_empty(), "יאללה should be in the lexicon");
        assert!(!h[0].en.to_lowercase().contains("o god"), "got {}", h[0].en);
    }
}
