//! phonetics.rs: sound-alike keys for Hebrew and Latin script, and a similarity score over them.
//! Public surface: key(word) -> String, similarity(a, b) -> f32.
//! Why this file: the defect class this app cannot otherwise see is the MISHEARD word - whisper
//!   returns something phonetically close and grammatically plausible, so no spell check flags it
//!   and no LLM cleanup fixes it ("comic push" for "commit and push", observed 2026-08-06). The
//!   only handle on it is sound: reduce both the transcript token and the speaker's known
//!   vocabulary to a pronunciation skeleton and measure the distance between them.
//! NOT responsible for: deciding what to do with a match (store.rs owns the mishearing table,
//!   translate.rs decides whether the model is told about it).
//! Test strategy: real pairs from live transcripts must score above the threshold
//!   (comic/commit, ריפו/repo), and unrelated same-length words must score below it. Both
//!   directions, or the threshold is decoration.

/// Collapse a word to a rough pronunciation skeleton.
///
/// This is a deliberately crude Metaphone: the goal is not linguistic accuracy, it is that two
/// words a Hebrew speaker would produce indistinguishably collapse to the same string. That is
/// why w and v merge (Hebrew has no /w/), th becomes t, and c becomes k or s by context - the
/// same substitutions the accent-repair prompt undoes, applied as a hash instead of as a fix.
pub fn key(word: &str) -> String {
    let w = word.trim().to_lowercase();
    if w.is_empty() {
        return String::new();
    }
    if w.chars().any(is_hebrew) {
        return hebrew_key(&w);
    }
    latin_key(&w)
}

fn is_hebrew(c: char) -> bool {
    ('\u{0590}'..='\u{05FF}').contains(&c)
}

fn latin_key(w: &str) -> String {
    let chars: Vec<char> = w.chars().filter(|c| c.is_alphanumeric()).collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or(' ');
        let mapped: Option<char> = match c {
            // Digraphs first - order matters, ph before p.
            'p' if next == 'h' => {
                i += 1;
                Some('f')
            }
            't' if next == 'h' => {
                i += 1;
                Some('t') // Hebrew speakers render both /θ/ and /ð/ as t or d
            }
            's' if next == 'h' => {
                i += 1;
                Some('x')
            }
            'c' if next == 'h' => {
                i += 1;
                Some('k')
            }
            'g' if next == 'h' => {
                i += 1;
                None // "night", "though" - silent
            }
            'c' => Some(if matches!(next, 'e' | 'i' | 'y') { 's' } else { 'k' }),
            'q' => Some('k'),
            'x' => Some('k'),
            'z' => Some('s'),
            // The labiovelar collapse: Hebrew has no /w/, so w and v are the same sound.
            'w' | 'v' => Some('v'),
            'g' | 'j' => Some('g'),
            'd' => Some('d'),
            'k' => Some('k'),
            'h' => None, // weakly articulated, frequently dropped or inserted
            'y' => None,
            // Vowels carry the least information and vary most by accent; keep only an initial
            // one so "ozen" and "zen" do not collapse together.
            'a' | 'e' | 'i' | 'o' | 'u' => {
                if out.is_empty() {
                    Some('a')
                } else {
                    None
                }
            }
            other if other.is_alphanumeric() => Some(other),
            _ => None,
        };
        if let Some(m) = mapped {
            // Collapse doubles: "commit" and "comit" must not differ.
            if out.chars().last() != Some(m) {
                out.push(m);
            }
        }
        i += 1;
    }
    out
}

fn hebrew_key(w: &str) -> String {
    let mut out = String::new();
    for (idx, c) in w.chars().enumerate() {
        let mapped = match c {
            // Final forms are the same letter.
            'ך' => Some('כ'),
            'ם' => Some('מ'),
            'ן' => Some('נ'),
            'ף' => Some('פ'),
            'ץ' => Some('צ'),
            // Pairs that are homophones in modern Israeli Hebrew.
            'ט' => Some('ת'),
            'ק' => Some('כ'),
            'ע' => Some('א'),
            'ש' => Some('ס'),
            'ב' => Some('ב'),
            // Matres lectionis: written vowels, dropped unless they open the word.
            'ו' | 'י' | 'ה' | 'א' => {
                if idx == 0 {
                    Some('א')
                } else {
                    None
                }
            }
            other if is_hebrew(other) => Some(other),
            other if other.is_alphanumeric() => Some(other),
            _ => None,
        };
        if let Some(m) = mapped {
            if out.chars().last() != Some(m) {
                out.push(m);
            }
        }
    }
    out
}

/// Below this length a word is not a candidate for phonetic matching at all.
///
/// This is a real boundary, not a tuning knob. Function words reduce to one or two symbols
/// ("the" -> t, "ze" -> s), so any threshold loose enough to pair them also pairs half the
/// lexicon with the other half. Short-word accent artifacts are handled where they can be
/// handled - by the accent-repair prompt, which has grammatical context and lists them
/// explicitly. This layer is for CONTENT words, where sound actually identifies the word.
pub const MIN_LEN: usize = 4;

/// 0..1 similarity between two words, by sound. Blends the phonetic-skeleton distance with the
/// raw spelling distance: skeletons alone are too permissive on short words (many three-letter
/// terms collapse together), spelling alone misses the accent substitutions entirely.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (ka, kb) = (key(a), key(b));
    if ka.is_empty() || kb.is_empty() {
        return 0.0;
    }
    let phonetic = ratio(&ka, &kb, sub_cost);
    let literal = ratio(&a.to_lowercase(), &b.to_lowercase(), |x, y| if x == y { 0.0 } else { 1.0 });
    phonetic * 0.7 + literal * 0.3
}

/// Substitution cost between two key symbols. Confusable pairs cost less than a full edit
/// because they are not really different sounds for this speaker: /θ/ and /ð/ surface as t, s,
/// d or z depending on the word, so "sink" and "tink" are both "think" and the key alone cannot
/// hold that. Pricing the confusion is what lets one key per word still work.
fn sub_cost(a: char, b: char) -> f32 {
    if a == b {
        return 0.0;
    }
    const CONFUSABLE: &[(char, char)] = &[
        ('t', 's'), // think -> tink | sink
        ('t', 'd'), // final devoicing
        ('t', 'x'), // th -> sh in some renderings
        ('s', 'x'), // s / sh
        ('v', 'b'), // Hebrew bet is both /b/ and /v/
        ('k', 'g'), // voicing
        ('f', 'p'),
    ];
    let hit = CONFUSABLE
        .iter()
        .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a));
    if hit {
        0.4
    } else {
        1.0
    }
}

fn ratio(a: &str, b: &str, cost: impl Fn(char, char) -> f32) -> f32 {
    let max = a.chars().count().max(b.chars().count());
    if max == 0 {
        return 0.0;
    }
    1.0 - (levenshtein(a, b, cost) / max as f32)
}

fn levenshtein(a: &str, b: &str, cost: impl Fn(char, char) -> f32) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len() as f32;
    }
    let mut prev: Vec<f32> = (0..=b.len()).map(|i| i as f32).collect();
    let mut cur = vec![0f32; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i as f32 + 1.0;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + cost(*ca, *cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1.0).min(cur[j] + 1.0);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live case this module exists for, plus its opposite. A threshold only means
    /// something if a real mishearing clears it AND an unrelated word of the same shape does
    /// not - otherwise every rare word gets "corrected" into a common one.
    #[test]
    fn real_mishearings_score_above_unrelated_words() {
        // Observed 2026-08-06: "commit and push" came back as "comic push".
        let hit = similarity("comic", "commit");
        let miss = similarity("comic", "branch");
        assert!(hit > 0.6, "comic/commit scored {hit}, expected > 0.6");
        assert!(miss < 0.4, "comic/branch scored {miss}, expected < 0.4");
        assert!(hit > miss + 0.25, "no separation: {hit} vs {miss}");
    }

    /// The accent substitutions must be invisible to the key, or the whole point is lost:
    /// these pairs are the SAME sound for a Hebrew speaker.
    #[test]
    fn accent_substitutions_collapse() {
        for (a, b) in [("vorld", "world"), ("tink", "think"), ("vant", "want"), ("sink", "think")] {
            let s = similarity(a, b);
            assert!(s > 0.7, "{a}/{b} scored {s}, expected > 0.7");
        }
    }

    /// The documented limit, pinned so nobody "fixes" it by loosening the threshold. /θ/ and
    /// /ð/ both surface as t, s, d or z depending on the word, and a 2-3 letter key cannot
    /// carry that ambiguity - "the" keys to t while "ze" keys to s. MIN_LEN keeps such words
    /// out of this layer entirely; the accent-repair prompt handles them with context.
    #[test]
    fn function_words_are_out_of_scope_by_length() {
        assert!("the".chars().count() < MIN_LEN);
        assert!("ze".chars().count() < MIN_LEN);
        assert!(similarity("ze", "the") < 0.6, "if this ever passes, MIN_LEN can be revisited");
    }

    /// Hebrew homophones and written-vowel variation must collapse too - ASR spells these
    /// inconsistently and they are the same spoken word.
    #[test]
    fn hebrew_spelling_variants_collapse() {
        for (a, b) in [("ריפו", "רפו"), ("קומיט", "כומיט"), ("טסט", "תסט")] {
            let s = similarity(a, b);
            assert!(s > 0.7, "{a}/{b} scored {s}, expected > 0.7");
        }
    }

    /// Guard against the degenerate direction: short unrelated words must NOT collapse just
    /// because vowels are dropped.
    #[test]
    fn short_unrelated_words_stay_apart() {
        assert!(similarity("push", "pull") < 0.65);
        assert!(similarity("test", "task") < 0.8);
        assert!(similarity("repo", "run") < 0.6);
    }
}
