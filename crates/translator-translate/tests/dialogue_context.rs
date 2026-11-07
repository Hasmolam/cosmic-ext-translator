use translator_translate::TranslationContext;

// -----------------------------------------------------------------------------
// Multi-turn sliding dialogue history and prompt generation
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
// Dialogue Context boundary capacities and stress rotation
// -----------------------------------------------------------------------------
#[test]
fn test_dialogue_context_boundary_capacities() {
    // Capacity 0: should drop all turns gracefully without panics
    let mut ctx_zero = TranslationContext::new(0);
    assert_eq!(ctx_zero.history().len(), 0);
    for i in 0..10 {
        ctx_zero.add_turn_with_speaker(
            Some("Speaker"),
            format!("Source {i}"),
            format!("Target {i}"),
        );
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
    assert!(
        pos_150 < pos_249,
        "Chronological order violation in system prompt"
    );
}
