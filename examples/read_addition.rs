use etpm::PackageManager;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("etpm=info".parse().unwrap()))
        .init();

    info!("Starting ETPM read_addition_file example");

    let mut manager = PackageManager::new();
    manager.set_root("./testroot_addition")?;
    manager.set_packages("./testroot_addition/packages")?;
    manager.add_repository("http://127.0.0.1:22869/")?;

    // Allow unsigned packages for local testing
    manager.set_allow_unsigned(true);

    // 1. Fetch the package (the .tp file is saved to disk, but not yet unpacked)
    info!("Fetching package...");
    let downloaded_path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;
    info!("Package downloaded to: {}", downloaded_path.display());

    // 2. Read the metadata file from the addition directory BEFORE installation
    // We expect the archive to contain a file at: package/addition/metadata.ron
    info!("Reading metadata from addition directory BEFORE installation...");

    let metadata_bytes = manager.read_addition_file(&downloaded_path, "metadata.ron")?;
    let metadata_str = String::from_utf8(metadata_bytes)?;

    info!("Raw metadata content:\n{}", metadata_str);

    // 3. Game-side validation logic (example)
    // In a real project, you would use serde + toml/json/ron to parse this strictly
    if metadata_str.contains("game_revision = \"1.5.2\"") {
        info!("Game revision matches! Proceeding with installation...");

        // 4. Install the package only if the validation check passes
        info!("Installing package...");
        manager
            .install_package(&downloaded_path, "example-package", "1.0.0")
            .await?;
        info!("Package installed successfully.");
    } else {
        warn!("Incompatible game revision detected in metadata! Aborting installation.");

        // Optional: clean up the downloaded file to save disk space
        std::fs::remove_file(&downloaded_path)?;
        info!("Cleaned up downloaded file.");
    }

    Ok(())
}
