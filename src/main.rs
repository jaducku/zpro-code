use clap::Parser;

use atom_code::services::api::openai_provider::OpenAiCompatibleProvider;
use atom_code::services::api::types::SystemBlock;
use atom_code::tools::get_builtin_tools;
use atom_code::tui::run::run_app;

#[derive(Parser)]
#[command(name = "atom", version)]
struct Cli {
    /// Model to use
    #[arg(long)]
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

    let provider = cli.api_base.map(|base| {
        let model = cli.model.unwrap_or_else(|| "default".into());
        Box::new(OpenAiCompatibleProvider::new(base, cli.api_key, model)) as Box<dyn atom_code::services::api::provider::LlmProvider>
    });

    let tools = get_builtin_tools();
    let system_prompt = cli.system_prompt.map(|s| vec![SystemBlock::Text { text: s }]);

    run_app(provider, tools, system_prompt).await?;

    Ok(())
}
