use std::collections::HashMap;
use translator_core::translation::{
    parse_speaker_tags, BracketStyle, ParsedDialogueLine, TranslationContext,
};

#[test]
fn test_speaker_parsing_nested_brackets() {
    let mut glossary = HashMap::new();
    glossary.insert("Hero".to_string(), "Kahraman".to_string());
    glossary.insert("Mage".to_string(), "Büyücü".to_string());

    // Case 1: Japanese lenticular containing square brackets: 【[Hero]】 Dialogue
    let line1 = parse_speaker_tags("【[Hero]】 Let's move forward!", &glossary);
    assert_eq!(line1.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(line1.raw_speaker.as_deref(), Some("[Hero]"));
    assert_eq!(line1.dialogue, "Let's move forward!");

    // Case 2: Square bracket containing lenticular: [【Mage】] Cast spell
    let line2 = parse_speaker_tags("[【Mage】] Cast spell", &glossary);
    assert_eq!(line2.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(line2.raw_speaker.as_deref(), Some("【Mage】"));
    assert_eq!(line2.dialogue, "Cast spell");

    // Case 3: Double square brackets: [[Hero]] Double protected
    let line3 = parse_speaker_tags("[[Hero]] Double protected", &glossary);
    assert_eq!(line3.bracket_style, Some(BracketStyle::SquareBracket));
    // The inner [ is captured as part of speaker nameplate
    assert_eq!(line3.raw_speaker.as_deref(), Some("[Hero"));
    assert_eq!(line3.dialogue, "] Double protected");

    // Case 4: Bracket followed by colon delimiter: [Hero]: Roger that!
    let line4 = parse_speaker_tags("[Hero]: Roger that!", &glossary);
    assert_eq!(line4.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(line4.raw_speaker.as_deref(), Some("Hero"));
    assert_eq!(line4.resolved_speaker.as_deref(), Some("Kahraman"));
    assert_eq!(line4.dialogue, "Roger that!");

    // Case 5: Japanese fullwidth colon following lenticular: 【Mage】： 了解。
    let line5 = parse_speaker_tags("【Mage】： 了解。", &glossary);
    assert_eq!(line5.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(line5.raw_speaker.as_deref(), Some("Mage"));
    assert_eq!(line5.resolved_speaker.as_deref(), Some("Büyücü"));
    assert_eq!(line5.dialogue, "了解。");
}

#[test]
fn test_speaker_parsing_unclosed_and_missing_brackets() {
    let glossary = HashMap::new();

    let unclosed_cases = [
        "【Missing close lenticular bracket and dialogue",
        "〔Missing close tortoise shell bracket and dialogue",
        "「Missing close corner bracket and dialogue",
        "[Missing close square bracket and dialogue",
        "［Missing close fullwidth square bracket and dialogue",
        "〖Missing close white lenticular bracket and dialogue",
    ];

    for unclosed in unclosed_cases {
        let parsed = parse_speaker_tags(unclosed, &glossary);
        // An unclosed bracket must NOT parse as a speaker tag
        assert!(
            parsed.bracket_style.is_none(),
            "Unclosed bracket must not have bracket_style for '{unclosed}'"
        );
        assert!(
            parsed.raw_speaker.is_none(),
            "Unclosed bracket must have raw_speaker == None for '{unclosed}'"
        );
        assert!(
            parsed.resolved_speaker.is_none(),
            "Unclosed bracket must have resolved_speaker == None for '{unclosed}'"
        );
        assert_eq!(
            parsed.dialogue, unclosed,
            "Dialogue must preserve the full string for unclosed bracket"
        );
    }
}

#[test]
fn test_speaker_parsing_empty_dialogue_and_corner_delimiters() {
    let mut glossary = HashMap::new();
    glossary.insert("Boss".to_string(), "Patron".to_string());

    // 1. Empty brackets: 【】 or []
    let empty_bracket1 = parse_speaker_tags("【】 Empty speaker here", &glossary);
    assert!(empty_bracket1.raw_speaker.is_none());
    assert_eq!(empty_bracket1.dialogue, "【】 Empty speaker here");

    let empty_bracket2 = parse_speaker_tags("[] Empty square speaker", &glossary);
    assert!(empty_bracket2.raw_speaker.is_none());
    assert_eq!(empty_bracket2.dialogue, "[] Empty square speaker");

    let whitespace_bracket = parse_speaker_tags("[   ] Whitespace speaker", &glossary);
    assert!(whitespace_bracket.raw_speaker.is_none());
    assert_eq!(whitespace_bracket.dialogue, "[   ] Whitespace speaker");

    // 2. Speaker tag without dialogue: 【Boss】
    // When speaker is in glossary, it should resolve speaker even with empty dialogue
    let boss_only = parse_speaker_tags("【Boss】", &glossary);
    assert_eq!(boss_only.raw_speaker.as_deref(), Some("Boss"));
    assert_eq!(boss_only.resolved_speaker.as_deref(), Some("Patron"));
    assert_eq!(boss_only.dialogue, "");

    // When speaker is NOT in glossary and dialogue is empty, it falls back to full string
    let unknown_only = parse_speaker_tags("【UnknownNPC】", &glossary);
    assert!(unknown_only.raw_speaker.is_none());
    assert_eq!(unknown_only.dialogue, "【UnknownNPC】");

    // 3. Colon delimiter corner cases
    // URL guard: http:// and https:// should NOT parse as speaker name
    let url_case = parse_speaker_tags("http://example.com/api", &glossary);
    assert!(url_case.raw_speaker.is_none());
    assert_eq!(url_case.dialogue, "http://example.com/api");

    // Timestamp guard: 12:34:56 should NOT parse as speaker name
    let time_case = parse_speaker_tags("12:34:56 system event", &glossary);
    assert!(time_case.raw_speaker.is_none());

    // Colon at pos 0: ": starting with colon"
    let zero_colon = parse_speaker_tags(": starting with colon", &glossary);
    assert!(zero_colon.raw_speaker.is_none());

    // Long prefix (>25 chars) before colon
    let long_prefix = "This prefix text is way too long to be a speaker name: Dialogue";
    let long_colon = parse_speaker_tags(long_prefix, &glossary);
    assert!(long_colon.raw_speaker.is_none());
    assert_eq!(long_colon.dialogue, long_prefix);

    // Multiline before colon:
    let multiline_colon = "First Line\nSecond Line: Dialogue";
    let multi_res = parse_speaker_tags(multiline_colon, &glossary);
    assert!(multi_res.raw_speaker.is_none());

    // Valid colon nameplate
    let valid_colon =
        parse_speaker_tags("Narrator: Once upon a time in a distant land...", &glossary);
    assert_eq!(
        valid_colon.bracket_style,
        Some(BracketStyle::ColonNameplate)
    );
    assert_eq!(valid_colon.raw_speaker.as_deref(), Some("Narrator"));
    assert_eq!(
        valid_colon.dialogue,
        "Once upon a time in a distant land..."
    );
}

// -----------------------------------------------------------------------------
// Bracket styles and speaker parsing
// -----------------------------------------------------------------------------
#[test]
fn test_all_bracket_styles_wrap_and_parse() {
    let mut glossary = HashMap::new();
    glossary.insert("アリス".to_string(), "Alice".to_string());
    glossary.insert("ボブ".to_string(), "Bob".to_string());
    glossary.insert("キャロル".to_string(), "Carol".to_string());
    glossary.insert("デイブ".to_string(), "Dave".to_string());
    glossary.insert("イブ".to_string(), "Eve".to_string());
    glossary.insert("フランク".to_string(), "Frank".to_string());
    glossary.insert("グレース".to_string(), "Grace".to_string());

    let cases = [
        (
            BracketStyle::Lenticular,
            "【アリス】 気をつけてね。",
            "アリス",
            "Alice",
            "気をつけてね。",
        ),
        (
            BracketStyle::TortoiseShell,
            "〔ボブ〕 急ごう！",
            "ボブ",
            "Bob",
            "急ごう！",
        ),
        (
            BracketStyle::CornerBracket,
            "「キャロル」 行きましょう。",
            "キャロル",
            "Carol",
            "行きましょう。",
        ),
        (
            BracketStyle::SquareBracket,
            "[デイブ] 了解した。",
            "デイブ",
            "Dave",
            "了解した。",
        ),
        (
            BracketStyle::FullwidthSquare,
            "［イブ］ 準備完了。",
            "イブ",
            "Eve",
            "準備完了。",
        ),
        (
            BracketStyle::WhiteLenticular,
            "〖フランク〗 これを見ろ。",
            "フランク",
            "Frank",
            "これを見ろ。",
        ),
        (
            BracketStyle::ColonNameplate,
            "グレース: 敵が現れた！",
            "グレース",
            "Grace",
            "敵が現れた！",
        ),
        (
            BracketStyle::ColonNameplate,
            "グレース： 敵が現れた！",
            "グレース",
            "Grace",
            "敵が現れた！",
        ),
    ];

    for (expected_style, raw_text, expected_raw, expected_resolved, expected_dialogue) in cases {
        let wrapped = expected_style.wrap(expected_raw);
        assert!(
            wrapped.contains(expected_raw),
            "Wrap for {:?} must contain speaker name",
            expected_style
        );

        let parsed = parse_speaker_tags(raw_text, &glossary);
        assert_eq!(
            parsed.bracket_style,
            Some(expected_style),
            "Failed style match for raw: '{raw_text}'"
        );
        assert_eq!(
            parsed.raw_speaker.as_deref(),
            Some(expected_raw),
            "Failed raw speaker match for: '{raw_text}'"
        );
        assert_eq!(
            parsed.resolved_speaker.as_deref(),
            Some(expected_resolved),
            "Failed resolved speaker match for: '{raw_text}'"
        );
        assert_eq!(
            parsed.dialogue, expected_dialogue,
            "Failed dialogue match for: '{raw_text}'"
        );
    }
}

#[test]
fn test_bracket_followed_by_colon_normalization() {
    let glossary = HashMap::new();
    let parsed1 = parse_speaker_tags("[Alice]: Hello everyone!", &glossary);
    assert_eq!(parsed1.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(parsed1.raw_speaker.as_deref(), Some("Alice"));
    assert_eq!(parsed1.dialogue, "Hello everyone!");

    let parsed2 = parse_speaker_tags("【Bob】：こんにちは！", &glossary);
    assert_eq!(parsed2.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(parsed2.raw_speaker.as_deref(), Some("Bob"));
    assert_eq!(parsed2.dialogue, "こんにちは！");
}

// -----------------------------------------------------------------------------
// Glossary resolution of character names
// -----------------------------------------------------------------------------
#[test]
fn test_glossary_resolution_exact_and_fallback() {
    let mut glossary = HashMap::new();
    glossary.insert("白夜".to_string(), "Byakuya".to_string());
    glossary.insert("赤井".to_string(), "Akai".to_string());

    // Character in glossary -> resolves to replacement
    let p1 = parse_speaker_tags("【白夜】 まだ終わっていない。", &glossary);
    assert_eq!(p1.raw_speaker.as_deref(), Some("白夜"));
    assert_eq!(p1.resolved_speaker.as_deref(), Some("Byakuya"));
    assert_eq!(p1.dialogue, "まだ終わっていない。");

    // Character not in glossary -> resolves to raw speaker name
    let p2 = parse_speaker_tags("【黒沢】 我々も行くぞ。", &glossary);
    assert_eq!(p2.raw_speaker.as_deref(), Some("黒沢"));
    assert_eq!(p2.resolved_speaker.as_deref(), Some("黒沢"));
    assert_eq!(p2.dialogue, "我々も行くぞ。");
}

// -----------------------------------------------------------------------------
// Speaker parsing adversarial edge cases
// -----------------------------------------------------------------------------
#[test]
fn test_speaker_parsing_adversarial_inputs() {
    let mut glossary = HashMap::new();
    glossary.insert("Hero".to_string(), "Kahraman".to_string());

    // 1. Empty and whitespace-only strings
    assert_eq!(
        parse_speaker_tags("", &glossary),
        ParsedDialogueLine {
            raw_speaker: None,
            resolved_speaker: None,
            dialogue: String::new(),
            bracket_style: None,
        }
    );
    assert_eq!(
        parse_speaker_tags("   \n\t  ", &glossary),
        ParsedDialogueLine {
            raw_speaker: None,
            resolved_speaker: None,
            dialogue: String::new(),
            bracket_style: None,
        }
    );

    // 2. Unclosed brackets
    let unclosed = parse_speaker_tags("【Alice Hello world", &glossary);
    assert_eq!(unclosed.bracket_style, None);
    assert_eq!(unclosed.dialogue, "【Alice Hello world");

    // 3. URLs and timestamps should not be parsed as speaker nameplates
    let url = parse_speaker_tags("https://github.com/system76/cosmic-epoch", &glossary);
    assert_eq!(url.bracket_style, None);
    assert_eq!(url.dialogue, "https://github.com/system76/cosmic-epoch");

    let timestamp = parse_speaker_tags("12:45 PM Meeting starts now", &glossary);
    assert_eq!(timestamp.bracket_style, None);

    // 4. Overly long colon prefix (> 25 characters) should not be parsed as speaker
    let long_prefix = "This is a very long descriptive clause that should not be a name: dialogue";
    let p_long = parse_speaker_tags(long_prefix, &glossary);
    assert_eq!(p_long.bracket_style, None);

    // 5. Colon prefix containing newline should not be parsed as speaker
    let newline_colon = "Line 1\nLine 2: dialogue";
    let p_nl = parse_speaker_tags(newline_colon, &glossary);
    assert_eq!(p_nl.bracket_style, None);

    // 6. Nested bracket quotes: speaker in lenticular, quote in corner brackets
    let nested = parse_speaker_tags("【Alice】 「Let's go together!」", &glossary);
    assert_eq!(nested.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(nested.raw_speaker.as_deref(), Some("Alice"));
    assert_eq!(nested.dialogue, "「Let's go together!」");

    // 7. Standalone bracket menu items (e.g. [OK] or [Cancel]) without dialogue
    let menu_button = parse_speaker_tags("[OK]", &glossary);
    assert_eq!(
        menu_button.bracket_style, None,
        "Standalone buttons should not be consumed as empty dialogue"
    );
    assert_eq!(menu_button.dialogue, "[OK]");

    // 8. Standalone bracket matching known character glossary
    let hero_tag = parse_speaker_tags("【Hero】", &glossary);
    assert_eq!(hero_tag.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(hero_tag.resolved_speaker.as_deref(), Some("Kahraman"));
    assert_eq!(hero_tag.dialogue, "");
}

// -----------------------------------------------------------------------------
// Speaker parsing nested, adjacent brackets and exotic unicode
// -----------------------------------------------------------------------------
#[test]
fn test_speaker_parsing_nested_and_adjacent_brackets() {
    let mut glossary = HashMap::new();
    glossary.insert("Alice".to_string(), "Aliye".to_string());
    glossary.insert("Bob".to_string(), "Bora".to_string());
    glossary.insert("Hero".to_string(), "Kahraman".to_string());

    // 1. Double bracket: [[Alice]] hello
    let p_double = parse_speaker_tags("[[Alice]] hello", &glossary);
    assert_eq!(p_double.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(p_double.raw_speaker.as_deref(), Some("[Alice"));
    assert_eq!(p_double.dialogue, "] hello");

    // 2. Adjacent brackets: [Alice][Bob] hello
    let p_adj = parse_speaker_tags("[Alice][Bob] hello", &glossary);
    assert_eq!(p_adj.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(p_adj.raw_speaker.as_deref(), Some("Alice"));
    assert_eq!(p_adj.resolved_speaker.as_deref(), Some("Aliye"));
    assert_eq!(p_adj.dialogue, "[Bob] hello");

    // 3. Mixed adjacent brackets: 【Alice】〔Bob〕 hello
    let p_mixed = parse_speaker_tags("【Alice】〔Bob〕 hello", &glossary);
    assert_eq!(p_mixed.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(p_mixed.raw_speaker.as_deref(), Some("Alice"));
    assert_eq!(p_mixed.resolved_speaker.as_deref(), Some("Aliye"));
    assert_eq!(p_mixed.dialogue, "〔Bob〕 hello");

    // 4. Bracket with colon and subtitle: [Alice: Leader] Forward!
    let p_sub = parse_speaker_tags("[Alice: Leader] Forward!", &glossary);
    assert_eq!(p_sub.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(p_sub.raw_speaker.as_deref(), Some("Alice: Leader"));
    assert_eq!(p_sub.dialogue, "Forward!");

    // 5. Fullwidth colon with extra fullwidth/halfwidth spaces: "Alice　：　Hello"
    let p_fw_spaces = parse_speaker_tags("Alice　：　Hello", &glossary);
    assert_eq!(
        p_fw_spaces.bracket_style,
        Some(BracketStyle::ColonNameplate)
    );
    assert_eq!(p_fw_spaces.raw_speaker.as_deref(), Some("Alice"));
    assert_eq!(p_fw_spaces.resolved_speaker.as_deref(), Some("Aliye"));
    assert_eq!(p_fw_spaces.dialogue, "Hello");

    // 6. Unopened brackets should not be treated as speaker tags
    let p_unopened = parse_speaker_tags("Alice] Hello", &glossary);
    assert_eq!(p_unopened.bracket_style, None);
    assert_eq!(p_unopened.dialogue, "Alice] Hello");

    // 7. Empty bracket pairs
    let p_empty_sq = parse_speaker_tags("[] Hello", &glossary);
    assert_eq!(p_empty_sq.bracket_style, None);
    assert_eq!(p_empty_sq.dialogue, "[] Hello");

    let p_empty_len = parse_speaker_tags("【】 Hello", &glossary);
    assert_eq!(p_empty_len.bracket_style, None);
    assert_eq!(p_empty_len.dialogue, "【】 Hello");

    // 8. Space-only brackets
    let p_space_bracket = parse_speaker_tags("【   】 Hello", &glossary);
    assert_eq!(p_space_bracket.bracket_style, None);
    assert_eq!(p_space_bracket.dialogue, "【   】 Hello");

    // 9. Emoji in speaker name
    let p_emoji = parse_speaker_tags("【👑Queen👑】 Bow before me!", &glossary);
    assert_eq!(p_emoji.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(p_emoji.raw_speaker.as_deref(), Some("👑Queen👑"));
    assert_eq!(p_emoji.dialogue, "Bow before me!");

    // 10. Non-Latin multilingual speakers
    let p_cjk = parse_speaker_tags("【魔王】 我が名は魔王。", &glossary);
    assert_eq!(p_cjk.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(p_cjk.raw_speaker.as_deref(), Some("魔王"));
    assert_eq!(p_cjk.dialogue, "我が名は魔王。");

    let p_arabic = parse_speaker_tags("【الملك】 مرحباً بك", &glossary);
    assert_eq!(p_arabic.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(p_arabic.raw_speaker.as_deref(), Some("الملك"));
    assert_eq!(p_arabic.dialogue, "مرحباً بك");

    // 11. Massive 5000-char speaker name inside brackets should parse safely without panic
    let huge_name = "X".repeat(5000);
    let huge_line = format!("【{huge_name}】 Speech");
    let p_huge = parse_speaker_tags(&huge_line, &glossary);
    assert_eq!(p_huge.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(p_huge.raw_speaker.as_deref(), Some(huge_name.as_str()));
    assert_eq!(p_huge.dialogue, "Speech");
}

// -----------------------------------------------------------------------------
// Glossary interception precedence and case-sensitivity
// -----------------------------------------------------------------------------
#[test]
fn test_glossary_interception_case_sensitivity_and_special_keys() {
    let mut glossary = HashMap::new();
    glossary.insert("Hero".to_string(), "Kahraman".to_string());
    glossary.insert("HERO".to_string(), "BÜYÜK KAHRAMAN".to_string());
    glossary.insert("Dr. Stone".to_string(), "Taş Doktoru".to_string());
    glossary.insert("[System]".to_string(), "Sistem".to_string());

    // Case sensitivity: "Hero" vs "hero" vs "HERO"
    let p_hero = parse_speaker_tags("【Hero】 Attack!", &glossary);
    assert_eq!(p_hero.resolved_speaker.as_deref(), Some("Kahraman"));

    let p_hero_caps = parse_speaker_tags("【HERO】 ATTACK!", &glossary);
    assert_eq!(
        p_hero_caps.resolved_speaker.as_deref(),
        Some("BÜYÜK KAHRAMAN")
    );

    let p_hero_lower = parse_speaker_tags("【hero】 attack!", &glossary);
    assert_eq!(p_hero_lower.resolved_speaker.as_deref(), Some("hero")); // not in glossary, falls back to raw

    // Speaker with punctuation
    let p_doc = parse_speaker_tags(
        "Dr. Stone: This is ten billion percent exciting!",
        &glossary,
    );
    assert_eq!(p_doc.resolved_speaker.as_deref(), Some("Taş Doktoru"));
    assert_eq!(p_doc.dialogue, "This is ten billion percent exciting!");

    // Context system prompt sorting with 100 glossary entries
    let mut ctx = TranslationContext::new(4);
    for i in (0..100).rev() {
        ctx.set_glossary_term(format!("Term_{:03}", i), format!("Çeviri_{:03}", i));
    }
    let prompt = ctx.build_system_prompt("en", "tr");
    let pos_000 = prompt.find("Term_000").unwrap();
    let pos_050 = prompt.find("Term_050").unwrap();
    let pos_099 = prompt.find("Term_099").unwrap();
    assert!(
        pos_000 < pos_050 && pos_050 < pos_099,
        "Glossary entries must be sorted alphabetically"
    );
}
