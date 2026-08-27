use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use keryx::config::Config;
use keryx::llm;
use keryx::paster;
use keryx::recorder::Recorder;
use keryx::transcriber;

#[tokio::main]
async fn main() {
    println!("=== Testing Keryx Complete Pipeline ===");
    let config = Config::load();
    println!("1. Config loaded:");
    println!("   STT Provider: {:?}", config.transcription_provider);
    println!("   LLM Provider: {:?}", config.llm_provider);

    println!("\n2. Testing Microphone recording for 2 seconds (generating 16kHz audio buffer)...");
    let rec = Recorder::new();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        stop_clone.store(true, Ordering::SeqCst);
    });

    let wav = tokio::task::spawn_blocking(move || {
        rec.record_with_streaming(stop, None)
    })
    .await
    .unwrap()
    .expect("Recording failed");

    println!("   Recorded {} bytes of WAV audio.", wav.len());
    assert!(wav.len() > 1000, "WAV audio is too short!");

    println!("\n3. Testing STT Transcription with whisper.cpp...");
    let raw_text = transcriber::transcribe(wav, &config)
        .await
        .expect("Transcription failed");
    println!("   Raw transcribed text: {:?}", raw_text);

    println!("\n4. Testing LLM Cleanup...");
    let clean_text = llm::post_process(&raw_text, &config)
        .await
        .expect("LLM processing failed");
    println!("   Cleaned text: {:?}", clean_text);

    println!("\n5. Testing Pasteboard & Keystroke Injection...");
    paster::paste_text("[Keryx Test Pipeline Check]")
        .expect("Pasting failed");
    println!("   ✓ Text copied to clipboard and Cmd+V dispatched!");

    println!("\n=== ALL 5 PIPELINE STAGES PASSED ===");
}
