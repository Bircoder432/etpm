# ETPM - Embeddable Tiny Package Manager

ETPM is a lightweight, embeddable package manager library written in Rust. It is designed to fetch and install packages from HTTP repositories. ETPM is not a full-featured package manager: it does not resolve dependencies or manage system-wide installations. Instead, it focuses on a single task: securely downloading a package archive, extracting it according to a strict layout, and tracking installed files for clean removal.

## Features

- **Minimalist & Focused**: Designed for monolithic, self-contained packages without complex dependency resolution.
- **Structured Archives**: Packages are strictly divided into `overlay` (files for the target system) and `addition` (package-specific metadata).
- **Automatic Tracking**: Generates a `filelist` upon installation, enabling safe, complete, and verifiable uninstallation.
- **Security First**: Built-in protection against Path Traversal (Tar Slip) attacks during archive extraction.
- **Async & Typed**: Fully asynchronous API powered by `tokio`, with comprehensive, typed error handling via `thiserror`.
- **Universal Embedding**: Provides a stable, safe **C FFI** (`etpm.h`), making it easy to embed ETPM into C, C++, Go, Python, Node.js, or any language with C-bindings support.

## Installation (Rust)

Add this to your `Cargo.toml`:

```toml
[dependencies]
etpm = { git = "https://codeberg.org/vstor08/etpm.git" }
tokio = { version = "1", features = ["full"] }
```

## Package Format

ETPM expects package archives (`.tp` files, which are standard `.tar.gz` archives) to have a strict internal directory structure:

```text
my-package-1.0.0.tp
└── package/
    ├── overlay/          # Contents are extracted to the `root` directory
    │   ├── usr/
    │   │   └── bin/
    │   │       └── my-app
    │   └── etc/
    │       └── my-app.conf
    └── addition/         # Contents are extracted to the `packages/<name>-<ver>` directory
        └── metadata.json
```

*Note: The top-level `package/` directory is automatically stripped during extraction.*

## Usage (Rust)

The following example demonstrates the complete lifecycle: configuring directories, fetching, installing, and uninstalling a package.

```rust
use etpm::PackageManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = PackageManager::new();

    // 1. Configure directories
    // `root` is where the `overlay/` contents will be extracted (e.g., system root or app dir)
    manager.set_root("/opt/my-application")?;
    
    // `packages` is where `addition/` contents and the `filelist` will be stored
    manager.set_packages("/var/lib/etpm")?;

    // 2. Add a repository
    manager.add_repository("https://repo.example.com/")?;

    // 3. Fetch the package archive to a temporary location
    let pkg_path = manager.fetch_package("my-app", "1.0.0", "/tmp").await?;

    // 4. Install the package
    // This extracts overlay to `root`, addition to `packages`, and generates `filelist`
    manager.install_package(&pkg_path, "my-app", "1.0.0").await?;
    println!("Package installed successfully.");

    // 5. Uninstall the package later
    // This reads the `filelist`, removes all tracked files from `root`, 
    // and deletes the package directory from `packages`.
    manager.uninstall_package("my-app", "1.0.0").await?;
    println!("Package uninstalled successfully.");

    Ok(())
}
```

## Embedding via C FFI

ETPM exposes a clean, memory-safe C API. You can find the `etpm.h` header file in the root of the repository. 

### Example (C)
```c
#include <stdio.h>
#include "etpm.h"

int main() {
    EtpmManager* manager = etpm_manager_new();
    
    etpm_set_root(manager, "/opt/my-application");
    etpm_set_packages(manager, "/var/lib/etpm");
    etpm_add_repository(manager, "https://repo.example.com/");

    char* pkg_path = NULL;
    if (etpm_fetch_package(manager, "my-app", "1.0.0", "/tmp", &pkg_path) == ETPM_OK) {
        printf("Downloaded to: %s\n", pkg_path);
        
        etpm_install_package(manager, pkg_path, "my-app", "1.0.0");
        etpm_free_string(pkg_path); // Free memory allocated by Rust
        
        etpm_uninstall_package(manager, "my-app", "1.0.0");
    } else {
        char* err = etpm_get_last_error(manager);
        printf("Error: %s\n", err);
        etpm_free_string(err);
    }

    etpm_manager_free(manager);
    return 0;
}
```
*(Bindings for Go, Python, and other languages can be easily written using this C API).*

## How Uninstallation Works

When `uninstall_package` is called, ETPM performs the following steps safely:
1. Locates the package directory at `$packages/<name>-<version>/`.
2. Reads the generated `filelist` file.
3. Iterates through the list, verifying each path against Path Traversal rules.
4. Removes each listed file from the `$root` directory (skipping missing files or directories gracefully to prevent errors).
5. Deletes the entire `$packages/<name>-<version>/` directory.

*Note: ETPM does not attempt to remove empty parent directories after file deletion, as they may contain files belonging to other packages or the host system.*

## Repository Format

A repository must contain an `index.ron` file at its root with the following structure:

```ron
(
    packages: {
        "my-app": [
            (version: "1.0.0", url: "my-app-1.0.0.tp"),
            (version: "1.1.0", url: "my-app-1.1.0.tp")
        ]
    }
)
```
The `url` field is relative to the repository URL. ETPM automatically joins the repository URL with this field to form the final download link.

## Building from Source

To build the Rust library and the C dynamic library (`cdylib`):
```bash
cargo build --release
```
The compiled library will be located in `target/release/` (e.g., `libetpm.so`, `libetpm.dylib`, or `etpm.dll`).

## License

ETPM is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
