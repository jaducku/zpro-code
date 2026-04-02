use clap::Parser;

use atom_code::services::api::openai_provider::OpenAiCompatibleProvider;
use atom_code::services::api::types::SystemBlock;
use atom_code::tools::get_builtin_tools;
use atom_code::tui::run::run_app;

#[derive(Parser)]
#[command(name = "atom-code", version)]
struct Cli {
    /// Initial prompt (non-interactive mode)
    prompt: Option<String>,

    /// Model to use
    #[arg(long, default_value = "default")]
    model: Option<String>,

    /// API base URL (e.g. https://openrouter.ai/api/v1)
    #[arg(long)]
    api_base: Option<String>,

    /// API key
    #[arg(long)]
    api_key: Option<String>,

    /// System prompt
    #[arg(long)]
    system_prompt: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Require api_base for interactive mode
    let Some(api_base) = cli.api_base else {
        println!("atom-code v{}", env!("CARGO_PKG_VERSION"));
        println!();
        println!("Usage:");
        println!("  atom-code --api-base <URL> --api-key <KEY> [--model <MODEL>]");
        println!();
        println!("Examples:");
        println!("  atom-code --api-base https://openrouter.ai/api/v1 --api-key sk-... --model anthropic/claude-sonnet-4-20250514");
        println!("  atom-code --api-base http://localhost:8000/v1 --model local-model");
        return Ok(());
    };

    let model = cli.model.unwrap_or_else(|| "default".into());
    let provider = OpenAiCompatibleProvider::new(api_base, cli.api_key, model);
    let tools = get_builtin_tools();

    let system_prompt = cli.system_prompt.map(|s| vec![SystemBlock::Text { text: s }]);

    run_app(Box::new(provider), tools, system_prompt).await?;

    Ok(())
}
