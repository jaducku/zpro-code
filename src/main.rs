use clap::Parser;

#[derive(Parser)]
#[command(name = "atom-code", version)]
struct Cli {
    /// Initial prompt (non-interactive mode)
    prompt: Option<String>,

    /// Model to use
    #[arg(long, default_value = "default")]
    model: Option<String>,

    /// API base URL
    #[arg(long)]
    api_base: Option<String>,

    /// API key
    #[arg(long)]
    api_key: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Display version if no interactive mode
    if cli.prompt.is_none() && cli.api_base.is_none() {
        println!("atom-code v{}", env!("CARGO_PKG_VERSION"));
        println!("Use --api-base and --api-key to connect to an LLM provider");
        return Ok(());
    }

    println!("atom-code v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
