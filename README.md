# ETPM - Embeddable Tiny Package Manager

ETPM (crate name: `etpm`) is a small, embeddable package manager library for Rust focused on securely downloading, verifying, and installing self-contained package archives from HTTP repositories. It is intentionally minimal — it does not perform dependency resolution or system package management — instead it provides a simple, auditable workflow for delivering application/artifact overlays and package metadata.

## Highlights

- Minimal, single-responsibility design.
- Packages are standard `tar.gz` archives with a strict layout (`package/overlay` + `package/addition`). Files in `overlay` are extracted into the configured `root`; files in `addition` are stored under the package directory and tracked.
- Cryptographic verification with Ed25519 signatures. Packages must include a Base64-encoded `.sig` file and are verified against registered trusted public keys.
- Safe unpacking: built-in Path Traversal (Tar Slip) protection.
- Fully asynchronous API powered by `tokio` and typed error handling via `thiserror`.
- C FFI (`cdylib`) for embedding in other languages — header available as `etpm.h`.
- Uses `tracing` macros for structured logging (library does not install a global subscriber; initialize logging in your application to see logs).

## Features

- Structured archives: `overlay` and `addition` directories inside each package.
- Signature verification using Ed25519 (multiple trusted keys supported).
- Atomic writes for downloaded files (temporary file renamed on successful verification).
- Filelist generation to support safe uninstall.
- C API for embedding in non-Rust environments.

## Installation (Rust)

From crates.io (recommended once published):

```toml
[dependencies]
etpm = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

Or use the current repository directly:

```toml
[dependencies]
etpm = { git = "https://codeberg.org/vstor08/etpm.git" }
```

If you want to see library logs, add `tracing-subscriber` to your application and initialize it (see Logging section).

## Logging

The library emits structured logs using the `tracing` crate. The library itself does NOT initialize a global subscriber — this should be done by the consuming application so it can control formatting and filtering.

Example minimal initialization in your binary:

```rust
use tracing_subscriber;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize a simple subscriber which respects RUST_LOG
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // ... start tokio runtime or call async main
    Ok(())
}
```

Then control log level with `RUST_LOG`, e.g. `RUST_LOG=info cargo run` or `RUST_LOG=debug cargo run`.

## Package format

ETPM expects `.tp` files to be standard `tar.gz` archives whose top-level directory is `package/` and which contain two subtrees:

```
package/
├── overlay/    # files to extract into the configured root
└── addition/   # files to place under packages/<name>-<version>/ (metadata, filelist, etc.)
```

The `package/` prefix is stripped during extraction.

## Repository layout and signatures

A repository must expose an `index.ron` file at its root describing available packages and versions. Each `url` field is relative to the repository root and the corresponding signature file must be available at `{url}.sig`.

Example repository layout:

```
repository/
├── index.ron
├── my-app-1.0.0.tp
├── my-app-1.0.0.tp.sig
├── my-app-1.1.0.tp
└── my-app-1.1.0.tp.sig
```

Example `index.ron` structure:

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

## Usage (Rust)

This example demonstrates initialization (including logging), adding a repository and a trusted key, fetching, installing and uninstalling a package.

```rust
use etpm::PackageManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize logging in the application if you want to see library logs
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut manager = PackageManager::new();
    manager.set_root("./packages")?;
    manager.set_packages("./packages")?;
    manager.add_repository("https://repo.example.com/")?;

    // register the Base64-encoded Ed25519 public key used for signing packages
    manager.add_trusted_key("REPLACE_WITH_BASE64_PUBLIC_KEY==")?;

    // fetch the package (downloads, verifies signature, returns local path)
    let package_path = manager.fetch_package("my-package", "1.0.0", "./downloads").await?;

    // install (unpacks overlay to root and addition to package dir)
    manager.install_package(&package_path, "my-package", "1.0.0").await?;

    // uninstall later if needed
    manager.uninstall_package("my-package", "1.0.0").await?;

    Ok(())
}
```

## API highlights

- `PackageManager::new()` — create a new manager instance.
- `set_root(path)` — set the installation root for `overlay` files.
- `set_packages(path)` — set the directory to store package metadata and filelists.
- `add_repository(url)` — add a repository URL (trailing slash will be normalized).
- `add_trusted_key(base64_pubkey)` — register a Base64-encoded Ed25519 public key for signature verification.
- `set_allow_unsigned(bool)` — allow downloading unsigned packages (use with caution).
- `fetch_package(name, version, dest)` — download and verify a package, returning the local path to the `.tp` file.
- `install_package(path, name, version)` — unpack and install the downloaded package.
- `uninstall_package(name, version)` — remove files recorded in the package `filelist` and delete the package directory.
- `check_update(name, current_version)` — asynchronously checks configured repositories for newer versions; returns `true` if an update exists.
- `get_latest_version(name)` — returns `Option<String>` with the latest version found across repositories.

## Embedding via C FFI

The C header `etpm.h` (in the repository root) describes the exported functions and status codes. Sample usage in C:

```c
#include <stdio.h>
#include "etpm.h"

int main() {
    EtpmManager* manager = etpm_manager_new();

    etpm_set_root(manager, "/opt/my-application");
    etpm_set_packages(manager, "/var/lib/etpm");
    etpm_add_repository(manager, "https://repo.example.com/");

    // Register trusted public key
    etpm_add_trusted_key(manager, "REPLACE_WITH_BASE64_PUBLIC_KEY==");

    char* pkg_path = NULL;
    EtpmStatus status = etpm_fetch_package(manager, "my-app", "1.0.0", "/tmp", &pkg_path);

    if (status == ETPM_OK) {
        printf("Downloaded & verified: %s\n", pkg_path);

        etpm_install_package(manager, pkg_path, "my-app", "1.0.0");
        etpm_free_string(pkg_path);

        etpm_uninstall_package(manager, "my-app", "1.0.0");
    } else if (status == ETPM_ERR_INVALID_SIGNATURE) {
        printf("Security error: package signature is invalid or missing!\n");
    } else {
        char* err = etpm_get_last_error(manager);
        printf("Error: %s\n", err);
        etpm_free_string(err);
    }

    etpm_manager_free(manager);
    return 0;
}
```

## Building from source

Build the Rust library and the C dynamic library (`cdylib`):

```bash
cargo build --release
```

Artifacts will be available in `target/release/` (e.g. `libetpm.so`, `libetpm.dylib`, `etpm.dll`).

## Notes on security

- Keep private signing keys secret. Only distribute public keys to clients that should trust packages.
- `set_allow_unsigned(true)` disables signature enforcement — use only in trusted environments.

## License

ETPM is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
