use screen_text_settle::{TextDiffAction, TextGate};

#[test]
fn test_adversarial_typewriter_1000_char_stream() {
    let mut gate = TextGate::new(0.92);

    // Generate a 1000-character sentence
    let mut text_chars = Vec::new();
    for i in 0..1000 {
        // Mix ASCII characters to create a realistic long narrative
        let ch = match i % 26 {
            0 => 'A',
            idx => (b'a' + (idx as u8)) as char,
        };
        text_chars.push(ch);
    }
    let full_string: String = text_chars.into_iter().collect();

    // Step 0: Initial character
    let mut current = String::new();
    current.push(full_string.chars().next().unwrap());
    let action0 = gate.evaluate(&current);
    assert_eq!(
        action0,
        TextDiffAction::EmitTranslation,
        "First character should emit translation"
    );
    assert_eq!(gate.last_emitted_text(), current);

    // Steps 1 to 999: Each step appends 1 character
    let chars: Vec<char> = full_string.chars().collect();
    for (i, &ch) in chars.iter().enumerate().skip(1) {
        current.push(ch);
        let action = gate.evaluate(&current);

        // Verification requirement: 1-character typewriter steps on long sentences
        // (sim > 0.98) are NEVER falsely suppressed as jitter!
        assert_eq!(
            action,
            TextDiffAction::TypewriterWait,
            "Step {i} (len={}): 1-char typewriter increment was falsely handled as {:?} instead of TypewriterWait",
            current.len(),
            action
        );
    }

    assert!(
        gate.has_pending_expansion(),
        "Should have pending expansion at the end of typewriter typing"
    );

    // Timeout settles -> flush pending
    let flushed = gate.flush_pending();
    assert_eq!(
        flushed,
        Some(full_string.clone()),
        "Flushed pending should match full 1000-char string"
    );
    assert_eq!(gate.last_emitted_text(), full_string);
    assert!(!gate.has_pending_expansion());
    assert_eq!(gate.flush_pending(), None);
}

#[test]
fn test_adversarial_typewriter_variable_pauses_and_duplicates() {
    let mut gate = TextGate::new(0.92);

    // Character typing with variable pauses
    assert_eq!(gate.evaluate("Hold"), TextDiffAction::EmitTranslation);

    // Quick typing (15ms)
    assert_eq!(gate.evaluate("Hold on"), TextDiffAction::TypewriterWait);

    // Exact duplicate frame while waiting on typewriter (game render cycle, no new letter yet)
    // Must return SuppressJitter WITHOUT modifying candidate text
    assert_eq!(gate.evaluate("Hold on"), TextDiffAction::SuppressJitter);
    assert_eq!(gate.last_candidate_text(), "Hold on");

    // Medium pause (150ms < 250ms settle)
    assert_eq!(
        gate.evaluate("Hold on, wait"),
        TextDiffAction::TypewriterWait
    );

    // Another duplicate
    assert_eq!(
        gate.evaluate("Hold on, wait"),
        TextDiffAction::SuppressJitter
    );

    // Final punctuation added
    assert_eq!(
        gate.evaluate("Hold on, wait!"),
        TextDiffAction::TypewriterWait
    );

    // Settle timeout passed: flushed
    assert_eq!(gate.flush_pending(), Some("Hold on, wait!".to_string()));
}

#[test]
fn test_adversarial_typewriter_japanese_multibyte_stream() {
    let mut gate = TextGate::new(0.92);

    // 1. Incremental progressive typing stream with multi-byte Japanese characters:
    // 「あ」 -> 「ありがとう」 -> 「ありがとうございます」
    // In actual game dialogue streaming, characters append as:
    // 「 -> 「あ -> 「あり -> 「ありが -> 「ありがと -> 「ありがとう -> ... -> 「ありがとうございます」
    let japanese_steps = [
        "「あ",
        "「あり",
        "「ありが",
        "「ありがと",
        "「ありがとう",
        "「ありがとうご",
        "「ありがとうござ",
        "「ありがとうござい",
        "「ありがとうございます",
        "「ありがとうございます」",
    ];

    // Initial character
    assert_eq!(
        gate.evaluate(japanese_steps[0]),
        TextDiffAction::EmitTranslation
    );

    // All subsequent characters must be TypewriterWait
    for (idx, step) in japanese_steps[1..].iter().enumerate() {
        let action = gate.evaluate(step);
        assert_eq!(
            action,
            TextDiffAction::TypewriterWait,
            "Japanese incremental step {idx} ('{step}') must be TypewriterWait, got {:?}",
            action
        );
    }

    assert_eq!(
        gate.flush_pending(),
        Some("「ありがとうございます」".to_string())
    );

    // 2. Coarse jumps as presented in the prompt:
    // 「あ」 -> 「ありがとう」 -> 「ありがとうございます」
    // Note: 「あ」ends with closing bracket '」', so 「ありがとう」 is NOT a prefix expansion of 「あ」.
    // It should cleanly trigger as distinct dialogue turns or updates without jitter suppression!
    let mut coarse_gate = TextGate::new(0.92);
    assert_eq!(
        coarse_gate.evaluate("「あ」"),
        TextDiffAction::EmitTranslation,
        "Initial Japanese text should emit"
    );

    // Jump from 「あ」 to 「ありがとう」:
    // Similarity is 3/7 = 0.42857 < 0.60 (turn_threshold)
    let act2 = coarse_gate.evaluate("「ありがとう」");
    assert_eq!(
        act2,
        TextDiffAction::EmitTranslation,
        "Jump from 「あ」 to 「ありがとう」 must emit translation (new dialogue text), got {:?}",
        act2
    );

    // Jump from 「ありがとう」 to 「ありがとうございます」:
    // Similarity is 7/12 = 0.58333 < 0.60 (turn_threshold)
    let act3 = coarse_gate.evaluate("「ありがとうございます」");
    assert_eq!(
        act3,
        TextDiffAction::EmitTranslation,
        "Jump from 「ありがとう」 to 「ありがとうございます」 must emit translation, got {:?}",
        act3
    );
}

#[test]
fn test_adversarial_1char_step_long_sentence_sim_above_098() {
    let mut gate = TextGate::new(0.92);

    // 70-character base sentence
    let base = "In the beginning the Universe was created. This made many very angry.";
    assert_eq!(gate.evaluate(base), TextDiffAction::EmitTranslation);
    assert_eq!(gate.flush_pending(), None);

    // 1-character additions (e.g. extending with 'a'):
    // base len = 69. Adding 'a' makes len 70.
    // Levenshtein similarity = 69 / 70 = 0.9857 > 0.98!
    // Without prefix expansion checked first, this would be falsely suppressed as jitter (0.9857 >= 0.92).
    let ext1 = format!("{base}a");
    let sim1 = strsim::normalized_levenshtein(&ext1, base);
    assert!(sim1 > 0.98, "Similarity must be > 0.98, got {sim1}");

    let action1 = gate.evaluate(&ext1);
    assert_eq!(
        action1,
        TextDiffAction::TypewriterWait,
        "1-char increment on long sentence (sim={sim1:.4} > 0.98) must NOT be suppressed as jitter!"
    );

    // Next 1-char addition: 'b'
    let ext2 = format!("{base}ab");
    let action2 = gate.evaluate(&ext2);
    assert_eq!(
        action2,
        TextDiffAction::TypewriterWait,
        "2nd 1-char increment on long sentence must NOT be suppressed as jitter!"
    );

    // Flush
    assert_eq!(gate.flush_pending(), Some(ext2));
}

#[test]
fn test_adversarial_random_mutations_vs_jitter_vs_new_turns() {
    let mut gate = TextGate::new(0.92);
    let base = "The quick brown fox jumps over the lazy dog near the riverbank.";
    assert_eq!(gate.evaluate(base), TextDiffAction::EmitTranslation);

    // Case 1: 1-character typo/OCR mutation in the middle (sim = 62/63 = 0.984 >= 0.92)
    // NOT a prefix expansion because middle character changed ("quick" -> "qu1ck")
    let mut middle_mutated = base.to_string();
    middle_mutated.replace_range(4..9, "qu1ck");
    let sim_mut = strsim::normalized_levenshtein(&middle_mutated, base);
    assert!(sim_mut >= 0.92, "Sim was {sim_mut}");
    assert_eq!(
        gate.evaluate(&middle_mutated),
        TextDiffAction::SuppressJitter,
        "Single-char OCR jitter in middle of sentence must be suppressed as jitter"
    );

    // Case 2: OCR punctuation drop at the end without prefix expansion (e.g. missing period)
    let no_period = &base[..base.len() - 1];
    let sim_no_period = strsim::normalized_levenshtein(no_period, base);
    assert!(sim_no_period >= 0.92);
    assert_eq!(
        gate.evaluate(no_period),
        TextDiffAction::SuppressJitter,
        "Dropped trailing punctuation must be suppressed as jitter"
    );

    // Case 3: Significant character changes (e.g. half the sentence replaced)
    let major_change = "The quick brown fox jumps into a deep dark cave in the forest.";
    let sim_major = strsim::normalized_levenshtein(major_change, base);
    assert!(sim_major < 0.92, "Sim was {sim_major}");
    assert_eq!(
        gate.evaluate(major_change),
        TextDiffAction::EmitTranslation,
        "Major sentence mutation must be accepted as new translation"
    );

    // Case 4: Completely distinct dialogue turn (sim < 0.60)
    let turn2 = "Farewell traveler, until we meet again at the castle gates!";
    let sim_turn2 = strsim::normalized_levenshtein(turn2, major_change);
    assert!(sim_turn2 < 0.60, "Sim was {sim_turn2}");
    assert_eq!(
        gate.evaluate(turn2),
        TextDiffAction::EmitTranslation,
        "Completely distinct turn must emit immediately"
    );
}

#[test]
fn test_adversarial_text_gate_unicode_and_fuzzing() {
    let mut gate = TextGate::new(0.92);

    // Exotic Unicode strings: CJK, Arabic RTL, Emojis, combining diacritics, zero-width chars
    let exotic_strings = [
        "",
        "   ",
        "\t\n  \r\n",
        "\u{3000}\u{3000}", // Fullwidth Japanese whitespace
        "こんにちは",
        "こんにちは世界",
        "مرحبا بالعالم",
        "مرحبا بك في اللعبة",
        "🎮 Level 1: Start Quest! 🗡️",
        "🎮 Level 1: Start Quest! 🗡️ Shield equipped.",
        "Z̸a̷l̶g̶o̶ ̴t̷e̷x̸t̷ ̷t̸e̶s̵t̵",
        "Z̸a̷l̶g̶o̶ ̴t̷e̷x̸t̷ ̷t̸e̶s̵t̵ extended",
    ];

    for (i, s) in exotic_strings.iter().enumerate() {
        // Must never panic on multibyte slicing or unusual codepoints
        let action = gate.evaluate(s);
        let _ = gate.has_pending_expansion();
        if i.is_multiple_of(3) {
            let _ = gate.flush_pending();
        }
        let _ = format!("{:?}", action);
    }

    // Reset works cleanly
    gate.reset();
    assert_eq!(gate.last_emitted_text(), "");
    assert_eq!(gate.last_candidate_text(), "");
    assert!(!gate.has_pending_expansion());
}

#[test]
fn test_adversarial_typewriter_resume_after_flush() {
    let mut gate = TextGate::new(0.92);

    // Turn 1 starts typing
    assert_eq!(
        gate.evaluate("First sentence part 1"),
        TextDiffAction::EmitTranslation
    );
    assert_eq!(
        gate.evaluate("First sentence part 1 and part 2"),
        TextDiffAction::TypewriterWait
    );

    // Settle timeout expires, flushed to HUD
    let flushed = gate.flush_pending();
    assert_eq!(
        flushed,
        Some("First sentence part 1 and part 2".to_string())
    );
    assert_eq!(gate.last_emitted_text(), "First sentence part 1 and part 2");

    // Game appends more text to the SAME dialogue without clearing:
    // (prefix expansion against last_emitted_text!)
    let extended = "First sentence part 1 and part 2 and bonus part 3!";
    let action_resumed = gate.evaluate(extended);
    assert_eq!(
        action_resumed,
        TextDiffAction::TypewriterWait,
        "Extension of settled text must trigger TypewriterWait"
    );

    assert_eq!(gate.flush_pending(), Some(extended.to_string()));
}
