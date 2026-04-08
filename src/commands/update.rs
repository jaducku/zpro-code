use crate::modules::update_manager::UpdateManager;
use anyhow::Result;
use std::path::PathBuf;

pub async fn handle_update_command(update_log_path: PathBuf) -> Result<()> {
    let manager = UpdateManager::new(update_log_path);

    println!("✓ Checking for updates...");

    match manager.check_latest_version().await? {
        Some(latest_version) => {
            println!("✓ Downloading v{}...", latest_version);
            println!("✓ Installing...");
            manager.install_update().await?;
            println!("✓ Updated to v{}", latest_version);
        }
        None => {
            println!("✓ Already on latest version");
        }
    }

    Ok(())
}
