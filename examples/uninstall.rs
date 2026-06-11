use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("etpm=info".parse().unwrap()))
        .init();

    info!("Starting ETPM download example");

    let mut manager = etpm::PackageManager::new();

    manager.set_root("./testroot")?;
    manager.set_packages("./packages")?;
    manager.add_repository("http://127.0.0.1:22869/")?;
    manager.set_allow_unsigned(true);

    info!("Repositories: {:?}", manager.list_repositories());

    let downloaded_path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;

    manager
        .install_package(&downloaded_path, "example-package", "1.0.0")
        .await?;

    info!("Package installed successfully.");
    Ok(())
}
