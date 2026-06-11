#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = etpm::PackageManager::new();
    manager.set_root("./testroot")?;
    manager.set_packages("./packages")?;
    manager.add_repository("http://127.0.0.1:22869/")?;

    println!("Repositories: {:?}", manager.list_repositories());

    // Initial installation
    let path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;
    manager
        .install_package(&path, "example-package", "1.0.0")
        .await?;

    // Check for updates
    let update: bool = manager.check_update("example-package", "1.0.0").await?;
    let last_version = manager
        .get_latest_version("example-package")
        .await?
        .unwrap_or_else(|| "unknown".to_string());

    if update {
        println!("Update available for 'example-package'!");
        let new_path = manager
            .fetch_package("example-package", &last_version, ".")
            .await?;
        manager
            .install_package(&new_path, "example-package", &last_version)
            .await?;
        println!("Package updated to version {}.", last_version);
    } else {
        println!("No update available for 'example-package'.");
    }

    Ok(())
}
