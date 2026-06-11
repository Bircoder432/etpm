use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = etpm::PackageManager::new();
    manager.set_root("./testroot")?;
    manager.set_packages("./packages")?;
    manager.add_repository("http://127.0.0.1:22869/")?;

    println!("Repositories: {:?}", manager.list_repositories());

    // Step 1: Install the package
    println!("\n=== Installing package ===");
    let path = manager
        .fetch_package("example-package", "1.0.0", ".")
        .await?;
    manager
        .install_package(&path, "example-package", "1.0.0")
        .await?;
    println!("Package installed successfully.");

    // Step 2: Show filelist content
    println!("\n=== Filelist content ===");
    let filelist_path = Path::new("./testroot/packages/example-package-1.0.0/filelist");
    if filelist_path.exists() {
        let content = std::fs::read_to_string(filelist_path)?;
        println!("{}", content);
    } else {
        println!("Filelist not found!");
    }

    // Step 3: Verify files exist
    println!("\n=== Verifying installed files ===");
    let test_file = Path::new("./testroot/test");
    if test_file.exists() {
        println!("✓ File exists: {}", test_file.display());
    } else {
        println!("✗ File missing: {}", test_file.display());
    }

    // Step 4: Uninstall the package
    println!("\n=== Uninstalling package ===");
    manager
        .uninstall_package("example-package", "1.0.0")
        .await?;
    println!("Package uninstalled successfully.");

    // Step 5: Verify files are removed
    println!("\n=== Verifying removal ===");
    if test_file.exists() {
        println!("✗ File still exists: {}", test_file.display());
    } else {
        println!("✓ File removed: {}", test_file.display());
    }

    let package_dir = Path::new("./testroot/packages/example-package-1.0.0");
    if package_dir.exists() {
        println!(
            "✗ Package directory still exists: {}",
            package_dir.display()
        );
    } else {
        println!("✓ Package directory removed: {}", package_dir.display());
    }

    // Step 6: Try to uninstall again (should fail)
    println!("\n=== Trying to uninstall again ===");
    match manager.uninstall_package("example-package", "1.0.0").await {
        Ok(_) => println!("Unexpected: uninstall succeeded"),
        Err(e) => println!("Expected error: {}", e),
    }

    Ok(())
}
