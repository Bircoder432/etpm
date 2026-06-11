#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create manager (default root = ".")
    let mut manager = etpm::PackageManager::new();

    // 2. Explicitly set root directory for installation
    manager.set_root("./testroot")?;

    // 3. Add repository
    manager.add_repository("http://127.0.0.1:22869/")?;

    println!("Repositories: {:?}", manager.list_repositories());

    // 4. Download and install
    let downloaded_path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;
    manager.install_package(&downloaded_path).await?;

    println!("Package installed successfully.");
    Ok(())
}
