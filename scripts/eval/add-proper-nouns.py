#!/usr/bin/env python3
"""Add the speaker's own product and tool names to the lexicon.

Why these and not a general name list: every entry below is either (a) a product he works on daily,
so it recurs in dictation, or (b) an observed failure in his live log. `רוזן -> Rosen` is the app
mishearing its own name; `פיירבול -> firewall` and `רלוד -> reroll` were caught by the gold-set
oracle on 2026-08-08. A proper noun is exactly the class a translator cannot infer and a lexicon
fixes at zero latency.

Idempotent: an entry whose `he` or variants already exist is skipped.
"""
from __future__ import annotations

import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
LEX = REPO / "docs/hebrew/lexicon.json"
PREFIXES = "הלבמושכ"

# (canonical hebrew, latin, [variants including observed mishearings], note)
NEW: list[tuple[str, str, list[str], str]] = [
    ("אוזן", "Ozen", ["רוזן", "אוזען", "עוזן"], "The app itself. Live log 2026-08-08: 'רוזן' shipped as 'Rosen'."),
    # Lookup is whole-token (see lexicon.rs `matching_is_whole_token`), so a key containing a
    # space can never match. Multi-word names are keyed on their joined and single-token forms.
    ("קלודקוד", "Claude Code", ["קלוד-קוד", "קלאודקוד"], "The agent he dictates into all day."),
    ("קלוד", "Claude", ["קלאוד"], "Said alone far more often than the full product name."),
    ("קולואנוס", "ColuanOS", ["קולונוס", "קלואנוס"], "His agent OS project."),
    ("מיינדטוטי", "Mind2t", ["מיינד2טי", "מיינדטו"], "His terminal/agent workbench."),
    ("טאורי", "Tauri", ["טאורי", "טאורים"], "Desktop framework used by Ozen itself."),
    ("אולמה", "Ollama", ["אולמה", "עולמה", "אולאמה"], "Local model runtime behind the translator."),
    ("ויספר", "Whisper", ["ויספר", "וויספר", "וויסper"], "The ASR model."),
    ("דיקטהלם", "DictaLM", ["דיקטהלם", "דיקטה אל אם", "דיקטלם"], "The Hebrew translator model."),
    ("אורב", "orb", ["האורב", "אורב"], "The floating window - his own word for it."),
    ("פיירבול", "fireball", ["פיירבול", "פייארבול"], "Gold set 2026-08-08: shipped as 'firewall'."),
    ("רילוד", "reload", ["רלוד", "רילורד", "רילואד"], "Gold set 2026-08-08: 'רילורד' shipped as 'reroll'."),
    ("אקסטנשן", "extension", ["אקסטינשן", "אקסטנשיין"], "Browser extension - recurs in his log."),
    ("סקרינשוט", "screenshot", ["סקרנישוט", "סקרינשוט", "סקרין שוט"], "Observed spelling in his log."),
    ("דפקט", "defect", ["דפקטים", "דיפקט"], "His standing word for a bug; he uses it constantly."),
    ("ריספונסיבי", "responsive", ["רספונסיבי", "ריספונסיב"], "UI vocabulary from his log."),
    ("פאנאאוט", "fanout", ["אפרוץ", "פאנאאוט"], "Gold set 2026-08-08: 'האפרוץ' shipped as 'break-in'."),
]


def prefixed(word: str) -> list[str]:
    """Hebrew glues ה/ל/ב/מ/ו/ש/כ straight onto a borrowed word, making one token."""
    return [p + word for p in PREFIXES]


def main() -> int:
    doc = json.loads(LEX.read_text())
    existing: set[str] = set()
    for e in doc["entries"]:
        if e.get("he"):
            existing.add(e["he"])
        existing.update(e.get("variants", []))
        forms = e.get("forms") or {}
        existing.update(forms.get("prefixed_hebrew_script", []))

    added = 0
    for he, latin, variants, note in NEW:
        if he in existing:
            print(f"  skip (present): {he}")
            continue
        keys = [he] + variants
        forms_prefixed: list[str] = []
        for k in keys:
            if " " not in k:
                forms_prefixed.extend(prefixed(k))
        doc["entries"].append(
            {
                "id": f"name:{latin.lower().replace(' ', '-')}",
                "class": "dev-transliteration",
                "he": he,
                "en": [latin],
                "latin": latin,
                "register": "dev",
                "variants": variants,
                "forms": {
                    "prefixed_hebrew_script": forms_prefixed,
                    "prefixed_latin_stem": [f"{p}-{latin}" for p in PREFIXES],
                },
                "notes": note,
                "source": "ozen live log + gold-set oracle, 2026-08-08 (Asia/Jerusalem)",
            }
        )
        added += 1

    doc["counts"]["dev-transliteration"] = sum(
        1 for e in doc["entries"] if e["class"] == "dev-transliteration"
    )
    doc["counts"]["total"] = len(doc["entries"])
    LEX.write_text(json.dumps(doc, ensure_ascii=False, indent=1))
    print(f"added {added} entries; total now {doc['counts']['total']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
