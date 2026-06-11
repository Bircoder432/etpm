# ETPM - Embeddable Tiny Package Manager

ETPM is a lightweight, embeddable package manager library written in Rust. It is designed to fetch and install packages from HTTP repositories. ETPM is not a full-featured package manager. It does not resolve dependencies or manage system-wide installations. Instead it focuses on one task: downloading a package archive from a repository and extracting it into a specified directory.

## Features

Minimalist design focused on a single responsibility. Packages are stored as tar.gz archives named in the format `name-version.tp`. The repository layout is static and indexed by an `index.ron` file. ETPM caches the index in memory and uses atomic file writes to prevent corruption. It performs basic security checks against path traversal attacks. The library is asynchronous and can be embedded into other Rust applications or exposed via a C API through cdylib.

## Installation

Add this to your Cargo.toml

```toml
[dependencies]
etpm = { git = "https://codeberg.org/vstor08/etpm.git" }
```

Or use it as a command-line tool after building from source

```bash
cargo build --release
```

## Usage

The following example demonstrates how to use ETPM as a library.

```rust
use etpm::PackageManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = PackageManager::new();
    manager.set_root("./packages")?;
    manager.add_repository("https://repo.example.com/")?;

    let package_path = manager.fetch_package("my-package", "1.0.0", "./downloads").await?;
    manager.install_package(&package_path).await?;

    Ok(())
}
```

The fetch_package method downloads the package archive into the specified destination directory. The install_package method extracts the archive into the root directory previously set with set_root. By default the root directory is the current working directory.

A repository must contain an index.ron file with the following structure

```ron
{
    "packages": {
        "my-package": [
            { "version": "1.0.0", "url": "my-package-1.0.0.tp" }
        ]
    }
}
```

The URL field is relative to the repository URL. ETPM joins the repository URL with the URL field to obtain the final download link.


## Embedding

ETPM is built primarily as a library. To use it from other languages a cdylib interface can be generated. The API exposes functions for adding repositories, fetching packages, and installing packages.

## License

ETPM is licensed under the MIT License. See the LICENSE file for details.
