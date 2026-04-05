/// Minimal test: call OpenRouter API and print the response.
/// Usage: cargo run --example test_api -- <api_base> <api_key> <model>
use zpro_code::services::api::openai_provider::OpenAiCompatibleProvider;
use zpro_code::services::api::provider::LlmProvider;
use zpro_code::services::api::types::*;
use zpro_code::types::content::ContentBlock;
use zpro_code::types::message::{ApiMessage, Role};
use futures::StreamExt;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: cargo run --example test_api -- <api_base> <api_key> <model>");
        eprintln!("Example: cargo run --example test_api -- https://openrouter.ai/api/v1 sk-... google/gemma-4-31b-it");
        std::process::exit(1);
    }

    let api_base = &args[1];
    let api_key = &args[2];
    let model = &args[3];

    println!("=== API Test ===");
    println!("Base: {}", api_base);
    println!("Model: {}", model);
    println!();

    let provider = OpenAiCompatibleProvider::new(
        api_base.clone(),
        Some(api_key.clone()),
        model.clone(),
    );

    let request = CreateMessageRequest {
        model: model.clone(),
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Say hello in one sentence.".to_string(),
            }],
        }],
        tools: None,
        max_tokens: 100,
        system: None,
        temperature: None,
        stream: false,
    };

    println!("[1] Sending non-streaming request...");
    match provider.create_message(request).await {
        Ok(mut stream) => {
            println!("[2] Got response stream, reading events...");
            while let Some(event) = stream.next().await {
                match event {
                    Ok(e) => println!("  Event: {:?}", e),
                    Err(e) => println!("  Error: {:?}", e),
                }
            }
            println!("[3] Stream ended.");
        }
        Err(e) => {
            println!("[ERROR] create_message failed: {:?}", e);
        }
    }
}
