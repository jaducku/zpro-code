use clap::Parser;

#[derive(Parser)]
#[command(name = "atom-code", version)]
struct Cli {
    /// Initial prompt
    prompt: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    println!("atom-code v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
