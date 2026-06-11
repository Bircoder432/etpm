use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("etpm=info".parse().unwrap()))
        .init();

    info!("Starting ETPM update example");

    let mut manager = etpm::PackageManager::new();
    manager.set_root("./testroot")?;
    manager.set_packages("./packages")?;
    manager.add_repository("http://127.0.0.1:22869/")?;
    manager.set_allow_unsigned(true);

    info!("Repositories: {:?}", manager.list_repositories());

    let path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;
    manager
        .install_package(&path, "example-package", "1.0.0")
        .await?;

    let update: bool = manager.check_update("example-package", "1.0.0").await?;
    let last_version = manager
        .get_latest_version("example-package")
        .await?
        .unwrap_or_else(|| "unknown".to_string());

    if update {
        info!("Update available for 'example-package'!");
        let new_path = manager
            .fetch_package("example-package", &last_version, ".")
            .await?;
        manager
            .install_package(&new_path, "example-package", &last_version)
            .await?;
        info!("Package updated to version {}.", last_version);
    } else {
        info!("No update available for 'example-package'.");
    }

    Ok(())
}
