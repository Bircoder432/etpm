#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = etpm::PackageManager::new();

    // Set root directory for overlay extraction
    manager.set_root("./testroot")?;

    // Set packages directory for metadata storage
    manager.set_packages("./packages")?;

    // Add repository
    manager.add_repository("http://127.0.0.1:22869/")?;

    manager.set_allow_unsigned(true);

    println!("Repositories: {:?}", manager.list_repositories());

    let downloaded_path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;

    manager
        .install_package(&downloaded_path, "example-package", "1.0.0")
        .await?;

    println!("Package installed successfully.");
    Ok(())
}
