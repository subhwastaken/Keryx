use keryx::config::{Config, TranscriptionProvider, LlmProvider, TtsProvider};
use keryx::smart_spacing::apply_smart_spacing;
use keryx::spoken_formatting::format_spoken_commands;
use keryx::recorder::resample_and_downmix;

#[test]
fn test_spoken_formatting_suffix_strip() {
    // Testing words ending in letters 'c', 'l', 'o', 's', 'e' to ensure strip_suffix does not truncate the word itself
    let input = "open quote close close quote";
    let formatted = format_spoken_commands(input);
    assert_eq!(formatted, "\"close\"");

    let input2 = "open parenthesis please enclose close parenthesis";
    let formatted2 = format_spoken_commands(input2);
    assert_eq!(formatted2, "(please enclose)");

    let input3 = "open bracket base case close bracket";
    let formatted3 = format_spoken_commands(input3);
    assert_eq!(formatted3, "[base case]");
}

#[test]
fn test_spoken_formatting_punctuation_commands() {
    let input = "hello world comma this is a test period new line how are you question mark";
    let formatted = format_spoken_commands(input);
    assert_eq!(formatted, "Hello world, this is a test.\nHow are you?");
}

#[test]
fn test_smart_spacing_rules() {
    // Case 1: Preceding character is a letter, incoming text starts with letter -> should add leading space
    let res = apply_smart_spacing("world", Some('o'));
    assert_eq!(res, " world");

    // Case 2: Preceding character is whitespace -> no extra space
    let res2 = apply_smart_spacing("world", Some(' '));
    assert_eq!(res2, "world");

    // Case 3: Preceding character is newline -> no extra space
    let res3 = apply_smart_spacing("world", Some('\n'));
    assert_eq!(res3, "world");

    // Case 4: Preceding character is opening bracket -> no extra space
    let res4 = apply_smart_spacing("world", Some('('));
    assert_eq!(res4, "world");

    // Case 5: Incoming text starts with punctuation -> no leading space
    let res5 = apply_smart_spacing(", hello", Some('o'));
    assert_eq!(res5, ", hello");
}

#[test]
fn test_utf8_multibyte_truncation() {
    // Multi-byte UTF-8 test with Japanese characters and emojis
    let long_utf8_text = "こんにちは世界！🚀🎉".repeat(10);
    let char_count = long_utf8_text.chars().count();
    assert!(char_count > 60);

    let truncated: String = long_utf8_text.chars().take(60).collect();
    assert_eq!(truncated.chars().count(), 60);
    // Ensure formatting with ellipsis produces a valid UTF-8 string without panics
    let formatted = format!("{}...", truncated);
    assert!(formatted.ends_with("..."));
}

#[test]
fn test_audio_resampling_stereo_to_mono() {
    // 48000 Hz Stereo -> 16000 Hz Mono (96000 samples total = 48000 frames of 2 channels)
    let input = vec![0.5f32; 96000]; // 1 sec stereo
    let output = resample_and_downmix(&input, 48000, 16000, 2);
    assert_eq!(output.len(), 16000);
    for &sample in &output {
        assert!((sample - 0.5f32).abs() < 1e-4);
    }
}

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.hotkey, "right_alt");
    assert_eq!(config.double_tap_ms, 400);
    assert_eq!(config.auto_stop_secs, 300);
    assert_eq!(config.transcription_provider, TranscriptionProvider::Auto);
    assert_eq!(config.llm_provider, LlmProvider::Nvidia);
    assert_eq!(config.ai_postprocessing, true);
    assert_eq!(config.tts_provider, TtsProvider::Auto);
}
