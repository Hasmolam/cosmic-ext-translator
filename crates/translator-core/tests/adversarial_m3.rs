use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

use image::{Rgba, RgbaImage};
use translator_core::capture::pipewire_thread::CapturedFrame;
use translator_core::dictionary::JapaneseDictionary;
use translator_core::ocr::MockOcrBackend;
use translator_core::orchestrator::{spawn_orchestrator, OrchestratorConfig, OrchestratorEvent};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::translation::{
    parse_speaker_tags, BoxFuture, BracketStyle, OfflineTranslationBackend,
    ParsedDialogueLine, RateLimitManager, TranslationBackend, TranslationContext,
};

// -----------------------------------------------------------------------------
// Test 1: Bracket styles and speaker parsing
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
        (BracketStyle::Lenticular, "【アリス】 気をつけてね。", "アリス", "Alice", "気をつけてね。"),
        (BracketStyle::TortoiseShell, "〔ボブ〕 急ごう！", "ボブ", "Bob", "急ごう！"),
        (BracketStyle::CornerBracket, "「キャロル」 行きましょう。", "キャロル", "Carol", "行きましょう。"),
        (BracketStyle::SquareBracket, "[デイブ] 了解した。", "デイブ", "Dave", "了解した。"),
        (BracketStyle::FullwidthSquare, "［イブ］ 準備完了。", "イブ", "Eve", "準備完了。"),
        (BracketStyle::WhiteLenticular, "〖フランク〗 これを見ろ。", "フランク", "Frank", "これを見ろ。"),
        (BracketStyle::ColonNameplate, "グレース: 敵が現れた！", "グレース", "Grace", "敵が現れた！"),
        (BracketStyle::ColonNameplate, "グレース： 敵が現れた！", "グレース", "Grace", "敵が現れた！"),
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
// Test 2: Glossary resolution of character names
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
// Test 3: Speaker parsing adversarial edge cases
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
// Test 4: Multi-turn sliding dialogue history and prompt generation
// -----------------------------------------------------------------------------

#[test]
fn test_sliding_dialogue_history_rotation_and_prompts() {
    let mut ctx = TranslationContext::new(3);
    assert_eq!(ctx.history().len(), 0);

    // Add turns with speakers
    ctx.add_turn_with_speaker(Some("Alice"), "Who goes there?", "Orada kim var?");
    ctx.add_turn_with_speaker(Some("Bob"), "A friend.", "Bir dost.");
    ctx.add_turn_with_speaker(None::<String>, "(Leaves rustle)", "(Yapraklar hışırdar)");

    assert_eq!(ctx.history().len(), 3);
    assert_eq!(ctx.history()[0].source, "(Leaves rustle)");
    assert_eq!(ctx.history()[1].source, "A friend.");
    assert_eq!(ctx.history()[2].source, "Who goes there?");

    // Add 4th turn -> 1st turn evicted (FIFO rotation)
    ctx.add_turn_with_speaker(Some("Alice"), "Step into the light.", "Işığa çık.");
    assert_eq!(ctx.history().len(), 3);
    assert_eq!(ctx.history()[0].source, "Step into the light.");
    assert_eq!(ctx.history()[1].source, "(Leaves rustle)");
    assert_eq!(ctx.history()[2].source, "A friend.");

    // Prompt construction
    ctx.set_glossary_term("Buster Sword", "Yıkıcı Kılıç");
    let prompt = ctx.build_system_prompt_with_speaker("en", "tr", Some("Bob"));

    // Check chronological order in prompt (oldest to newest)
    let pos_friend = prompt.find("Bob: \"A friend.\" => \"Bir dost.\"");
    let pos_rustle = prompt.find("Dialogue: \"(Leaves rustle)\" => \"(Yapraklar hışırdar)\"");
    let pos_step = prompt.find("Alice: \"Step into the light.\" => \"Işığa çık.\"");

    assert!(pos_friend.is_some());
    assert!(pos_rustle.is_some());
    assert!(pos_step.is_some());
    assert!(pos_friend.unwrap() < pos_rustle.unwrap());
    assert!(pos_rustle.unwrap() < pos_step.unwrap());

    // Check speaker and glossary metadata
    assert!(prompt.contains("Current Line Speaker: \"Bob\""));
    assert!(prompt.contains("- \"Buster Sword\" -> \"Yıkıcı Kılıç\""));

    // Clear history
    ctx.clear_history();
    assert_eq!(ctx.history().len(), 0);
}

// -----------------------------------------------------------------------------
// Test 5: Rate limit exponential backoff math and pause duration
// -----------------------------------------------------------------------------

#[test]
fn test_rate_limit_exponential_backoff_progression_and_caps() {
    let mut rlm = RateLimitManager::default(); // 30s base, max exponent 6
    assert_eq!(rlm.base_pause(), Duration::from_secs(30));
    assert_eq!(rlm.max_exponent(), 6);
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.consecutive_errors(), 0);

    // Consecutive errors: 30 * 2^k
    let expected_durations = [
        30,   // 30 * 2^0
        60,   // 30 * 2^1
        120,  // 30 * 2^2
        240,  // 30 * 2^3
        480,  // 30 * 2^4
        960,  // 30 * 2^5
        1920, // 30 * 2^6
        1920, // capped at 2^6
        1920, // capped at 2^6
    ];

    for (idx, &expected_secs) in expected_durations.iter().enumerate() {
        let pause = rlm.record_rate_limit();
        assert_eq!(
            pause,
            Duration::from_secs(expected_secs),
            "Failed at error index {}",
            idx
        );
        assert!(rlm.is_throttled());
        assert!(rlm.remaining_pause().is_some());
        assert!(rlm.remaining_pause().unwrap() <= Duration::from_secs(expected_secs));
    }

    assert_eq!(rlm.consecutive_errors(), 9);

    // Recovery upon successful translation
    rlm.record_success();
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.consecutive_errors(), 0);
    assert_eq!(rlm.remaining_pause(), None);
}

// -----------------------------------------------------------------------------
// Test 6: Japanese morphological dictionary greedy segmentation
// -----------------------------------------------------------------------------

#[test]
fn test_japanese_dictionary_greedy_longest_match_and_pos() {
    let mut dict = JapaneseDictionary::empty();

    dict.insert("魔法", "まほう", "Noun", vec!["magic".to_string()]);
    dict.insert(
        "魔法使い",
        "まほうつかい",
        "Noun",
        vec!["magician".to_string(), "wizard".to_string()],
    );
    dict.insert("剣", "つるぎ", "Noun", vec!["sword".to_string()]);
    dict.insert(
        "聖剣",
        "せいけん",
        "Noun",
        vec!["holy sword".to_string()],
    );
    dict.insert(
        "装備",
        "そうび",
        "Noun, Suru Verb",
        vec!["equipment".to_string(), "gear".to_string()],
    );

    // Test greedy match: "魔法使い" (len 4) should match instead of "魔法" (len 2)
    let cards = dict.analyze("魔法使いが聖剣を装備した");
    let surfaces: Vec<&str> = cards.iter().map(|c| c.surface.as_str()).collect();

    assert_eq!(surfaces, vec!["魔法使い", "聖剣", "装備"]);
    assert_eq!(cards[0].reading, "まほうつかい");
    assert_eq!(cards[1].reading, "せいけん");
    assert_eq!(cards[2].reading, "そうび");
    assert_eq!(cards[0].definitions, vec!["magician", "wizard"]);

    // Test unknown chars & empty strings
    let empty_cards = dict.analyze("");
    assert!(empty_cards.is_empty());

    let unknown_cards = dict.analyze("1234567890");
    assert!(unknown_cards.is_empty());

    // Test lookup helper
    let lookup_res = dict.lookup("聖剣");
    assert!(lookup_res.is_some());
    assert_eq!(lookup_res.unwrap().reading, "せいけん");
}

// -----------------------------------------------------------------------------
// Test 7: Offline translation backend local execution
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_offline_translation_engine_rules_and_local_paths() {
    let backend = OfflineTranslationBackend::new("nllb-200-distilled-600M")
        .with_model_dir("/var/models/translation")
        .with_rule("Attack", "Saldır")
        .with_rule("Defend", "Savun");

    assert_eq!(backend.model_name(), "nllb-200-distilled-600M");
    assert_eq!(
        backend.model_path(),
        std::path::PathBuf::from("/var/models/translation/nllb-200-distilled-600M")
    );

    let mut ctx = TranslationContext::default();
    ctx.set_glossary_term("Warrior", "Savaşçı");

    // Exact rule translation
    let res_exact = backend.translate("Attack", "en", "tr", &ctx).await.unwrap();
    assert_eq!(res_exact, "Saldır");

    // Substring rule & glossary translation
    let res_sub = backend
        .translate("Warrior Defend now!", "en", "tr", &ctx)
        .await
        .unwrap();
    assert!(res_sub.contains("Savaşçı"));
    assert!(res_sub.contains("Savun"));
    assert!(res_sub.contains("[Offline: tr]"));
}

// -----------------------------------------------------------------------------
// Test 8: Orchestrator speaker tag preservation and HTTP 429 rate limit backoff
// -----------------------------------------------------------------------------

struct RateLimitFailingBackend {
    call_count: Arc<AtomicUsize>,
    should_fail_429: bool,
}

impl TranslationBackend for RateLimitFailingBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _src_lang: &'a str,
        _target_lang: &'a str,
        _ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let fail = self.should_fail_429;
        let text = text.to_string();

        Box::pin(async move {
            if fail {
                anyhow::bail!("HTTP 429 Rate Limit Exceeded: quota depleted");
            }
            // Confirm speaker was stripped from dialogue text passed to backend
            assert!(
                !text.contains('【') && !text.contains('】'),
                "TranslationBackend received raw speaker brackets instead of clean dialogue!"
            );
            Ok(format!("Translated: {text}"))
        })
    }
}

fn make_test_frame_with_blocks(num_blocks: u32) -> CapturedFrame {
    let mut img = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
    for i in 0..num_blocks {
        let start_x = (i * 12).min(90);
        for y in 10..40 {
            for x in start_x..(start_x + 8).min(100) {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }
    CapturedFrame {
        image: img,
        timestamp: Instant::now(),
        original_width: 100,
        original_height: 50,
    }
}

#[tokio::test]
async fn test_orchestrator_speaker_tag_preservation_and_turn_recording() {
    let (frame_tx, frame_rx) = watch::channel(None);
    let ocr_backend = Arc::new(MockOcrBackend::new("【アリス】 気をつけてね。"));
    let call_count = Arc::new(AtomicUsize::new(0));
    let translation_backend = Arc::new(RateLimitFailingBackend {
        call_count: Arc::clone(&call_count),
        should_fail_429: false,
    });

    let debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(40),
        Duration::from_millis(400),
    );

    let mut glossary = HashMap::new();
    glossary.insert("アリス".to_string(), "Alice".to_string());

    let config = OrchestratorConfig {
        forward_to_hud: false,
        glossary,
        typewriter_settle_duration: Duration::from_millis(50),
        ..Default::default()
    };

    let mut handle = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    let event = handle.next_event().await.unwrap();
    assert_eq!(event, OrchestratorEvent::Started);

    // Push frames to settle debouncer
    frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();

    // Expect translation event
    let mut got_translation = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady { original, translated, cached })) =
            tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "【アリス】 気をつけてね。");
            // Check that output preserved the speaker tag with glossary replacement
            assert!(
                translated.contains("【Alice】"),
                "Output '{translated}' did not preserve re-wrapped speaker tag!"
            );
            assert!(
                translated.contains("Translated: 気をつけてね。"),
                "Output '{translated}' did not translate dialogue properly!"
            );
            assert!(!cached);
            got_translation = true;
            break;
        }
    }
    assert!(got_translation, "Expected TranslationReady event with speaker preservation");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    handle.stop();
}

#[tokio::test]
async fn test_orchestrator_rate_limit_backoff_blocks_further_dispatches() {
    let (frame_tx, frame_rx) = watch::channel(None);
    let ocr_backend = Arc::new(MockOcrBackend::new("Text that triggers 429"));
    let call_count = Arc::new(AtomicUsize::new(0));
    let translation_backend = Arc::new(RateLimitFailingBackend {
        call_count: Arc::clone(&call_count),
        should_fail_429: true,
    });

    let debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(40),
        Duration::from_millis(400),
    );

    let config = OrchestratorConfig {
        forward_to_hud: false,
        typewriter_settle_duration: Duration::from_millis(50),
        ..Default::default()
    };

    let mut handle = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    let _ = handle.next_event().await; // Started

    // Trigger frame settle
    frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(3))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(3))).unwrap();

    // Wait for Error event
    let mut got_error = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::Error(err))) =
            tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert!(err.contains("429"));
            got_error = true;
            break;
        }
    }
    assert!(got_error, "Expected OrchestratorEvent::Error with 429");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // Trigger another frame settle while rate-limited (different block count to animate then settle)
    frame_tx.send(Some(make_test_frame_with_blocks(1))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(1))).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Backend must NOT be called again because RateLimitManager throttles it!
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "Backend was called while rate limit throttle was active!"
    );

    handle.stop();
}

// -----------------------------------------------------------------------------
// Test 9: Speaker parsing nested, adjacent brackets and exotic unicode
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
    assert_eq!(p_fw_spaces.bracket_style, Some(BracketStyle::ColonNameplate));
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
// Test 10: Dialogue Context boundary capacities and stress rotation
// -----------------------------------------------------------------------------

#[test]
fn test_dialogue_context_boundary_capacities() {
    // Capacity 0: should drop all turns gracefully without panics
    let mut ctx_zero = TranslationContext::new(0);
    assert_eq!(ctx_zero.history().len(), 0);
    for i in 0..10 {
        ctx_zero.add_turn_with_speaker(Some("Speaker"), format!("Source {i}"), format!("Target {i}"));
    }
    assert_eq!(ctx_zero.history().len(), 0);
    let prompt_zero = ctx_zero.build_system_prompt_with_speaker("ja", "en", Some("CurrentSpeaker"));
    assert!(prompt_zero.contains("Current Line Speaker: \"CurrentSpeaker\""));
    assert!(!prompt_zero.contains("[Previous Dialogue Turns"));

    // Capacity 1: retains only the latest turn
    let mut ctx_one = TranslationContext::new(1);
    ctx_one.add_turn_with_speaker(Some("Alice"), "Line 1", "Satır 1");
    assert_eq!(ctx_one.history().len(), 1);
    assert_eq!(ctx_one.history()[0].source, "Line 1");

    ctx_one.add_turn_with_speaker(Some("Bob"), "Line 2", "Satır 2");
    assert_eq!(ctx_one.history().len(), 1);
    assert_eq!(ctx_one.history()[0].source, "Line 2");
    assert_eq!(ctx_one.history()[0].speaker.as_deref(), Some("Bob"));

    let prompt_one = ctx_one.build_system_prompt_with_speaker("en", "tr", None);
    assert!(prompt_one.contains("Bob: \"Line 2\" => \"Satır 2\""));
    assert!(!prompt_one.contains("Line 1"));

    // Capacity 100: stress test with 250 turns
    let mut ctx_large = TranslationContext::new(100);
    for i in 0..250 {
        let spk = if i % 2 == 0 { "Alice" } else { "Bob" };
        ctx_large.add_turn_with_speaker(
            Some(spk),
            format!("Question {i} from {spk}"),
            format!("Answer {i} from {spk}"),
        );
    }
    assert_eq!(ctx_large.history().len(), 100);

    // Oldest retained is turn 150, newest is turn 249
    assert_eq!(ctx_large.history()[0].source, "Question 249 from Bob");
    assert_eq!(ctx_large.history()[99].source, "Question 150 from Alice");

    let prompt_large = ctx_large.build_system_prompt_with_speaker("en", "tr", Some("Alice"));
    assert!(prompt_large.contains("Question 150 from Alice"));
    assert!(prompt_large.contains("Question 249 from Bob"));
    assert!(!prompt_large.contains("Question 149 from Bob")); // evicted!

    // Verify chronological ordering in prompt
    let pos_150 = prompt_large.find("Question 150").unwrap();
    let pos_249 = prompt_large.find("Question 249").unwrap();
    assert!(pos_150 < pos_249, "Chronological order violation in system prompt");
}

// -----------------------------------------------------------------------------
// Test 11: Glossary interception precedence and case-sensitivity
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
    assert_eq!(p_hero_caps.resolved_speaker.as_deref(), Some("BÜYÜK KAHRAMAN"));

    let p_hero_lower = parse_speaker_tags("【hero】 attack!", &glossary);
    assert_eq!(p_hero_lower.resolved_speaker.as_deref(), Some("hero")); // not in glossary, falls back to raw

    // Speaker with punctuation
    let p_doc = parse_speaker_tags("Dr. Stone: This is ten billion percent exciting!", &glossary);
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
    assert!(pos_000 < pos_050 && pos_050 < pos_099, "Glossary entries must be sorted alphabetically");
}

// -----------------------------------------------------------------------------
// Test 12: RateLimitManager high-count stress and cap guarantees
// -----------------------------------------------------------------------------

#[test]
fn test_rate_limit_manager_stress_and_caps() {
    let mut rlm = RateLimitManager::new(Duration::from_secs(5), 4); // 5s * 2^4 = 80s cap

    // 100 consecutive errors should not overflow or exceed 80s
    for _ in 0..100 {
        let pause = rlm.record_rate_limit();
        assert!(pause <= Duration::from_secs(80));
    }
    assert_eq!(rlm.consecutive_errors(), 100);
    assert!(rlm.is_throttled());

    // Remaining pause must be <= 80s
    let rem = rlm.remaining_pause().unwrap();
    assert!(rem <= Duration::from_secs(80));

    // Recovery
    rlm.record_success();
    assert_eq!(rlm.consecutive_errors(), 0);
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.remaining_pause(), None);
}

// -----------------------------------------------------------------------------
// Test 13: Japanese dictionary throughput and longest match prefix stress
// -----------------------------------------------------------------------------

#[test]
fn test_japanese_dictionary_stress_and_longest_prefix() {
    let mut dict = JapaneseDictionary::empty();
    dict.insert("超", "ちょう", "Prefix", vec!["super".to_string()]);
    dict.insert("魔法", "まほう", "Noun", vec!["magic".to_string()]);
    dict.insert("大魔法", "だいまほう", "Noun", vec!["great magic".to_string()]);
    dict.insert("超大魔法", "ちょうだいまほう", "Noun", vec!["super great magic".to_string()]);

    // Longest match: "超大魔法" should be chosen over "超", "大魔法", "魔法"
    let cards = dict.analyze("超大魔法を発動した！");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].surface, "超大魔法");
    assert_eq!(cards[0].reading, "ちょうだいまほう");

    // Stress test: 50,000 characters of mixed text
    let core_dict = JapaneseDictionary::new();
    let repeat_text = "勇者が魔法で攻撃する。私と敵と剣と盾。大丈夫ありがとう。".repeat(1000);
    let start = Instant::now();
    let analyzed = core_dict.analyze(&repeat_text);
    let elapsed = start.elapsed();

    assert!(!analyzed.is_empty());
    assert!(elapsed < Duration::from_millis(500), "Dictionary analysis took too long: {:?}", elapsed);
}

// -----------------------------------------------------------------------------
// Test 14: Orchestrator cache decoupling across multiple distinct speakers
// -----------------------------------------------------------------------------

struct CountingMultiSpeakerBackend {
    calls: Arc<AtomicUsize>,
}

impl TranslationBackend for CountingMultiSpeakerBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _src_lang: &'a str,
        _target_lang: &'a str,
        _ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = text.to_string();
        Box::pin(async move {
            Ok(format!("TR:{text}"))
        })
    }
}

#[derive(Debug)]
struct DynamicMockOcrBackend {
    texts: std::sync::Mutex<std::collections::VecDeque<String>>,
}

impl DynamicMockOcrBackend {
    fn new(texts: Vec<&str>) -> Self {
        Self {
            texts: std::sync::Mutex::new(texts.into_iter().map(|s| s.to_string()).collect()),
        }
    }
}

impl translator_core::ocr::OcrBackend for DynamicMockOcrBackend {
    fn recognize<'a>(
        &'a self,
        _image: &'a image::DynamicImage,
    ) -> translator_core::ocr::BoxFuture<'a, anyhow::Result<String>> {
        let mut guard = self.texts.lock().unwrap();
        let text = guard.pop_front().unwrap_or_default();
        Box::pin(async move { Ok(text) })
    }

    fn recognize_detailed<'a>(
        &'a self,
        _image: &'a image::DynamicImage,
        _orientation: Option<translator_core::ocr::TextOrientation>,
    ) -> translator_core::ocr::BoxFuture<'a, anyhow::Result<translator_core::ocr::OcrResult>> {
        let mut guard = self.texts.lock().unwrap();
        let text = guard.pop_front().unwrap_or_default();
        Box::pin(async move {
            Ok(translator_core::ocr::OcrResult {
                text,
                boxes: Vec::new(),
                orientation: translator_core::ocr::TextOrientation::Horizontal,
            })
        })
    }
}

#[tokio::test]
async fn test_orchestrator_multi_speaker_cache_decoupling() {
    let (frame_tx, frame_rx) = watch::channel(None);
    let calls = Arc::new(AtomicUsize::new(0));
    let ocr_backend = Arc::new(DynamicMockOcrBackend::new(vec![
        "【Alice】 Good morning!",
        "【Bob】 Good morning!",
        "Good morning!",
    ]));
    let translation_backend = Arc::new(CountingMultiSpeakerBackend {
        calls: Arc::clone(&calls),
    });

    let debouncer = FrameDebouncer::new(
        2,
        Duration::from_millis(40),
        Duration::from_millis(400),
    );

    let mut glossary = HashMap::new();
    glossary.insert("Alice".to_string(), "Aliye".to_string());
    glossary.insert("Bob".to_string(), "Bora".to_string());

    let config = OrchestratorConfig {
        forward_to_hud: false,
        glossary,
        typewriter_settle_duration: Duration::from_millis(50),
        ..Default::default()
    };

    let mut handle = spawn_orchestrator(
        frame_rx,
        debouncer,
        ocr_backend,
        translation_backend,
        config,
    );

    let _ = handle.next_event().await; // Started

    // Settle 1st frame with Alice
    frame_tx.send(Some(make_test_frame_with_blocks(0))).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(2))).unwrap();

    let mut got_alice = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady { original, translated, cached })) =
            tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "【Alice】 Good morning!");
            assert_eq!(translated, "【Aliye】 TR:Good morning!");
            assert!(!cached);
            got_alice = true;
            break;
        }
    }
    assert!(got_alice, "Expected Alice translation");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Settle 2nd frame with Bob (different blocks -> trigger OCR for Bob)
    frame_tx.send(Some(make_test_frame_with_blocks(4))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(4))).unwrap();

    let mut got_bob = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady { original, translated, cached })) =
            tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "【Bob】 Good morning!");
            assert_eq!(translated, "【Bora】 TR:Good morning!");
            assert!(cached, "Bob translation must be a CACHE HIT!");
            got_bob = true;
            break;
        }
    }
    assert!(got_bob, "Expected Bob translation via cache hit");
    // Call count must STILL be 1 because it was a cache hit!
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Settle 3rd frame without speaker (different blocks -> trigger OCR for narrator)
    frame_tx.send(Some(make_test_frame_with_blocks(6))).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    frame_tx.send(Some(make_test_frame_with_blocks(6))).unwrap();

    let mut got_narrator = false;
    for _ in 0..10 {
        if let Ok(Some(OrchestratorEvent::TranslationReady { original, translated, cached })) =
            tokio::time::timeout(Duration::from_millis(300), handle.next_event()).await
        {
            assert_eq!(original, "Good morning!");
            assert_eq!(translated, "TR:Good morning!");
            assert!(cached, "Narrator translation must also be a CACHE HIT!");
            got_narrator = true;
            break;
        }
    }
    assert!(got_narrator, "Expected Narrator translation via cache hit");
    // Call count must STILL be 1!
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    handle.stop();
}


// -----------------------------------------------------------------------------
// Test 9: Adversarial RateLimitManager exponential progression (k = 0..=20) and recovery
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_rate_limit_exponential_overflow_and_recovery() {
    let mut rlm = RateLimitManager::default(); // 30s base, max exponent 6
    assert_eq!(rlm.base_pause(), Duration::from_secs(30));
    assert_eq!(rlm.max_exponent(), 6);
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.consecutive_errors(), 0);

    // Test consecutive errors k = 0, 1, 2, ..., 20
    for k in 0..=20u32 {
        let expected_exponent = k.min(6);
        let expected_secs = 30u64 * (1u64 << expected_exponent);
        let expected_duration = Duration::from_secs(expected_secs);

        let pause = rlm.record_rate_limit();

        // 1. Assert exact mathematical formula: 30s * 2^(min(k, 6))
        assert_eq!(
            pause, expected_duration,
            "Rate limit pause mismatch at k={k}: got {:?}, expected {:?}",
            pause, expected_duration
        );

        // 2. Assert capped at 1920s for all k >= 6
        if k >= 6 {
            assert_eq!(
                pause,
                Duration::from_secs(1920),
                "Pause must remain capped at 1920s for k >= 6 (k={k})"
            );
        }

        // 3. Assert consecutive errors tracking
        assert_eq!(rlm.consecutive_errors(), k + 1);

        // 4. Assert throttling status
        assert!(rlm.is_throttled());
        let remaining = rlm.remaining_pause().expect("Expected active remaining pause");
        assert!(remaining <= expected_duration);
    }

    // Stress beyond k=20 up to k=100: ensure no integer overflow or unexpected multipliers
    for k in 21..=100u32 {
        let pause = rlm.record_rate_limit();
        assert_eq!(
            pause,
            Duration::from_secs(1920),
            "Cap at 1920s must hold without overflow up to k={k}"
        );
        assert_eq!(rlm.consecutive_errors(), k + 1);
        assert!(rlm.is_throttled());
    }

    // Recovery test: record_success must reset error count to 0 and clear pause
    rlm.record_success();
    assert!(!rlm.is_throttled(), "Must not be throttled after record_success");
    assert_eq!(rlm.consecutive_errors(), 0, "consecutive_errors must be 0 after record_success");
    assert!(rlm.remaining_pause().is_none(), "remaining_pause must be None after record_success");

    // Immediately trigger rate limit again -> must restart at base pause 30s (30 * 2^0)
    let fresh_pause = rlm.record_rate_limit();
    assert_eq!(fresh_pause, Duration::from_secs(30));
    assert_eq!(rlm.consecutive_errors(), 1);
    assert!(rlm.is_throttled());
}

// -----------------------------------------------------------------------------
// Test 10: Concurrent race condition and recovery stress on RateLimitManager
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_rate_limit_concurrent_stress_and_recovery() {
    let rlm = Arc::new(tokio::sync::Mutex::new(RateLimitManager::default()));
    let num_tasks = 20;
    let mut handles = Vec::new();

    // 20 concurrent tasks recording rate limits
    for _ in 0..num_tasks {
        let rlm_clone = Arc::clone(&rlm);
        handles.push(tokio::spawn(async move {
            let mut mgr = rlm_clone.lock().await;
            mgr.record_rate_limit()
        }));
    }

    for h in handles {
        let pause = h.await.unwrap();
        assert!(pause >= Duration::from_secs(30));
        assert!(pause <= Duration::from_secs(1920));
    }

    {
        let mgr = rlm.lock().await;
        assert_eq!(mgr.consecutive_errors(), num_tasks);
        assert!(mgr.is_throttled());
    }

    // One thread records success
    {
        let mut mgr = rlm.lock().await;
        mgr.record_success();
        assert_eq!(mgr.consecutive_errors(), 0);
        assert!(!mgr.is_throttled());
        assert!(mgr.remaining_pause().is_none());
    }

    // Subsequent call starts fresh
    {
        let mut mgr = rlm.lock().await;
        let p = mgr.record_rate_limit();
        assert_eq!(p, Duration::from_secs(30));
        assert_eq!(mgr.consecutive_errors(), 1);
    }
}

// -----------------------------------------------------------------------------
// Test 11: RateLimitManager edge-case configurations & truncation behavior
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_rate_limit_edge_configurations() {
    // 1. max_exponent = 0: pause is always base_pause
    let mut rlm_zero_exp = RateLimitManager::new(Duration::from_millis(500), 0);
    for _ in 0..10 {
        assert_eq!(rlm_zero_exp.record_rate_limit(), Duration::from_millis(500));
    }

    // 2. Millisecond base pause with exponential scaling
    let mut rlm_ms = RateLimitManager::new(Duration::from_millis(100), 3);
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(100)); // 100 * 2^0
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(200)); // 100 * 2^1
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(400)); // 100 * 2^2
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(800)); // 100 * 2^3
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(800)); // capped

    // 3. Documented architectural vulnerability observation:
    // If max_exponent >= 32, (1u64 << 32 as u32) truncates to 0!
    let mut rlm_32 = RateLimitManager::new(Duration::from_secs(1), 32);
    for _ in 0..32 {
        rlm_32.record_rate_limit();
    }
    let truncated_pause = rlm_32.record_rate_limit();
    // Multiplier truncates to 0, producing Duration::ZERO
    assert_eq!(truncated_pause, Duration::ZERO, "Confirmed u32 truncation to 0 when exponent >= 32");

    // 4. In debug mode, max_exponent >= 64 triggers shift overflow panic on 1u64 << 64
    let panic_result = std::panic::catch_unwind(|| {
        let mut rlm_64 = RateLimitManager::new(Duration::from_secs(1), 64);
        for _ in 0..64 {
            rlm_64.record_rate_limit();
        }
        rlm_64.record_rate_limit();
    });
    assert!(panic_result.is_err(), "Confirmed shift overflow panic when max_exponent >= 64");
}

// -----------------------------------------------------------------------------
// Test 12: Japanese morphological dictionary exhaustive adversarial tests
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_japanese_dictionary_boundaries_and_collisions() {
    let mut dict = JapaneseDictionary::new();

    // 1. Empty string, whitespace, tabs, newlines
    assert!(dict.analyze("").is_empty(), "Empty string must return empty Vec");
    assert!(dict.analyze("   \n\t  \r\n").is_empty(), "Whitespace must return empty Vec");

    // 2. Single characters
    // 2a. Non-Japanese ASCII
    assert!(dict.analyze("x").is_empty());
    assert!(dict.analyze("9").is_empty());
    assert!(dict.analyze("\0").is_empty());

    // 2b. Japanese kana not in dictionary
    assert!(dict.analyze("あ").is_empty());
    assert!(dict.analyze("ん").is_empty());

    // 2c. Single-character kanji IN core dictionary
    let res_watashi = dict.analyze("私");
    assert_eq!(res_watashi.len(), 1);
    assert_eq!(res_watashi[0].surface, "私");
    assert_eq!(res_watashi[0].reading, "わたし");
    assert_eq!(res_watashi[0].part_of_speech, "Pronoun");
    assert_eq!(res_watashi[0].definitions, vec!["I", "me"]);

    let res_ken = dict.analyze("剣");
    assert_eq!(res_ken.len(), 1);
    assert_eq!(res_ken[0].surface, "剣");
    assert_eq!(res_ken[0].reading, "つるぎ");
    assert_eq!(res_ken[0].part_of_speech, "Noun");

    let res_teki = dict.analyze("敵");
    assert_eq!(res_teki.len(), 1);
    assert_eq!(res_teki[0].surface, "敵");
    assert_eq!(res_teki[0].reading, "てき");

    let res_tate = dict.analyze("盾");
    assert_eq!(res_tate.len(), 1);
    assert_eq!(res_tate[0].surface, "盾");
    assert_eq!(res_tate[0].reading, "たて");

    // 3. Non-Japanese text
    assert!(dict.analyze("The quick brown fox jumps over the lazy dog.").is_empty());
    assert!(dict.analyze("1234567890 !?@#$%^&*()_+-=[]{}|;':\",./<>?").is_empty());
    assert!(dict.analyze("🚀🎮🔥✨ Привет мир 안녕하세요").is_empty());

    // Mixed non-Japanese with Japanese core vocabulary
    let mixed = dict.analyze("Player 1 [LV 99] 勇者 used 魔法! 敵 took 999 DMG.");
    let mixed_surfaces: Vec<&str> = mixed.iter().map(|c| c.surface.as_str()).collect();
    assert_eq!(mixed_surfaces, vec!["勇者", "魔法", "敵"]);

    // 4. Unknown kanji
    assert!(dict.analyze("魑魅魍魎跋扈").is_empty(), "Unknown kanji must return empty Vec");
    assert!(dict.analyze("麒麟と鳳凰").is_empty());

    // Unknown kanji adjoining known vocabulary
    let adjoining = dict.analyze("大勇者が魔神を討伐する");
    let adjoining_surfaces: Vec<&str> = adjoining.iter().map(|c| c.surface.as_str()).collect();
    assert_eq!(adjoining_surfaces, vec!["勇者"], "Must isolate known words despite unknown kanji prefixes/suffixes");

    // 5. Overlapping dictionary entries / prefix collisions
    // Add "魔法使い" (4 chars) to overlap with "魔法" (2 chars)
    dict.insert(
        "魔法使い",
        "まほうつかい",
        "Noun",
        vec!["wizard".to_string(), "sorcerer".to_string()],
    );

    // 5a. Prefix collision: "魔法使い" must match longest entry (4 chars), NOT "魔法" (2 chars)
    let coll1 = dict.analyze("魔法使い");
    assert_eq!(coll1.len(), 1);
    assert_eq!(coll1[0].surface, "魔法使い");
    assert_eq!(coll1[0].reading, "まほうつかい");

    // 5b. Short entry alone: "魔法" must still match "魔法"
    let coll2 = dict.analyze("魔法");
    assert_eq!(coll2.len(), 1);
    assert_eq!(coll2[0].surface, "魔法");

    // 5c. Sequence of short and long entries
    let coll3 = dict.analyze("魔法使いは魔法を放った");
    let coll3_surfaces: Vec<&str> = coll3.iter().map(|c| c.surface.as_str()).collect();
    assert_eq!(coll3_surfaces, vec!["魔法使い", "魔法"]);

    // 5d. Multi-level nested prefixes
    dict.insert("東", "ひがし", "Noun", vec!["East".to_string()]);
    dict.insert("東京", "とうきょう", "Noun", vec!["Tokyo".to_string()]);
    dict.insert("東京都", "とうきょうと", "Noun", vec!["Tokyo Metropolis".to_string()]);
    dict.insert(
        "東京都知事",
        "とうきょうとちじ",
        "Noun",
        vec!["Governor of Tokyo".to_string()],
    );

    assert_eq!(dict.analyze("東京都知事")[0].surface, "東京都知事");
    assert_eq!(dict.analyze("東京都")[0].surface, "東京都");
    assert_eq!(dict.analyze("東京")[0].surface, "東京");
    assert_eq!(dict.analyze("東")[0].surface, "東");

    let nested_sentence = dict.analyze("東京都知事が東京の東へ移動した");
    let nested_surfaces: Vec<&str> = nested_sentence.iter().map(|c| c.surface.as_str()).collect();
    assert_eq!(nested_surfaces, vec!["東京都知事", "東京", "東"]);

    // 6. Zero division, zero indexing panics, extreme inputs
    // Inserting empty string must not cause infinite loop or panic
    dict.insert("", "", "", vec![]);
    assert_eq!(dict.analyze("勇者").len(), 1);
    assert!(dict.analyze("").is_empty());

    // Null bytes inside text
    let null_text = dict.analyze("勇者\0\0魔法");
    let null_surfaces: Vec<&str> = null_text.iter().map(|c| c.surface.as_str()).collect();
    assert_eq!(null_surfaces, vec!["勇者", "魔法"]);

    // Unicode combining characters (dakuten combining mark)
    let combining = dict.analyze("勇者\u{304B}\u{3099}魔法");
    let comb_surfaces: Vec<&str> = combining.iter().map(|c| c.surface.as_str()).collect();
    assert_eq!(comb_surfaces, vec!["勇者", "魔法"]);

    // High volume stress: 10,000 characters
    let repeated = "勇者が魔法で攻撃し、防御する。".repeat(500);
    let start = Instant::now();
    let stress_cards = dict.analyze(&repeated);
    let elapsed = start.elapsed();
    assert_eq!(stress_cards.len(), 500 * 4); // 4 words per sentence ("勇者", "魔法", "攻撃", "防御")
    assert!(elapsed < Duration::from_millis(500), "10k char analysis took {:?}", elapsed);

    // 7. 16-character sliding window limitation boundary test
    let entry_16 = "一二三四五六七八九十一二三四五六"; // 16 chars
    let entry_17 = "一二三四五六七八九十一二三四五六七"; // 17 chars
    dict.insert(entry_16, "reading", "Noun", vec!["16 chars".to_string()]);
    dict.insert(entry_17, "reading", "Noun", vec!["17 chars".to_string()]);

    let res_16 = dict.analyze(entry_16);
    assert_eq!(res_16.len(), 1, "16-character morpheme must be matched");
    assert_eq!(res_16[0].surface, entry_16);

    let res_17 = dict.analyze(entry_17);
    // 17-character morpheme cannot be matched as a whole because analyze caps window at 16 chars!
    // Instead it matches the 16-character prefix entry_16!
    assert_eq!(res_17.len(), 1);
    assert_eq!(res_17[0].surface, entry_16, "17-char word falls back to 16-char prefix due to min(16) window cap");
}
