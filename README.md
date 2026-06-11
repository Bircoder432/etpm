
# ETPM - Embeddable Tiny Package Manager

ETPM is a lightweight, embeddable package manager library written in Rust. It is designed to fetch and install packages from HTTP repositories. ETPM is not a full-featured package manager: it does not resolve dependencies or manage system-wide installations. Instead, it focuses on a single task: securely downloading a package archive, verifying its authenticity via Ed25519 signatures, extracting it according to a strict layout, and tracking installed files for clean removal.

## Features

- **Minimalist & Focused**: Designed for monolithic, self-contained packages without complex dependency resolution.
- **Structured Archives**: Packages are strictly divided into `overlay` (files for the target system) and `addition` (package-specific metadata).
- **Cryptographic Verification**: All packages are signed with Ed25519. ETPM verifies signatures against a set of trusted public keys before installation, ensuring authenticity and integrity.
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

## Signature Verification

ETPM enforces signature verification for all packages. Each package archive must have a corresponding `.sig` file containing a Base64-encoded Ed25519 signature.

### Repository Layout with Signatures

```text
repository/
├── index.ron
├── my-app-1.0.0.tp
├── my-app-1.0.0.tp.sig    # Base64-encoded Ed25519 signature
├── my-app-1.1.0.tp
└── my-app-1.1.0.tp.sig
```

### How Verification Works

1. ETPM downloads the `.tp` archive to a temporary file.
2. ETPM downloads the corresponding `.sig` file.
3. The archive content is verified against the signature using one of the registered trusted public keys.
4. If verification succeeds, the temporary file is atomically renamed to the final path.
5. If verification fails, the temporary file is deleted and an `InvalidSignature` error is returned.

### Generating Keys and Signing Packages

**Generate a keypair** (using Python with PyNaCl):
```python
import nacl.signing
import base64

signing_key = nacl.signing.SigningKey.generate()
verify_key = signing_key.verify_key

print("Private Key (keep secret!):", base64.b64encode(signing_key.encode()).decode())
print("Public Key (distribute!):", base64.b64encode(verify_key.encode()).decode())
```

**Sign a package**:
```python
import nacl.signing
import base64

signing_key = nacl.signing.SigningKey(base64.b64decode("YOUR_PRIVATE_KEY"))

with open("my-app-1.0.0.tp", "rb") as f:
    package_data = f.read()

signature = signing_key.sign(package_data).signature

with open("my-app-1.0.0.tp.sig", "w") as f:
    f.write(base64.b64encode(signature).decode())
```

## Usage (Rust)

The following example demonstrates the complete lifecycle: configuring directories, registering a trusted key, fetching, installing, and uninstalling a package.

```rust
use etpm::PackageManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = PackageManager::new();

    // 1. Configure directories
    manager.set_root("/opt/my-application")?;
    manager.set_packages("/var/lib/etpm")?;

    // 2. Add a repository
    manager.add_repository("https://repo.example.com/")?;

    // 3. Register a trusted public key (Base64-encoded Ed25519)
    // This key MUST match the one used to sign packages in the repository.
    let trusted_public_key = "REPLACE_WITH_BASE64_PUBLIC_KEY==";
    manager.add_trusted_key(trusted_public_key)?;

    // 4. Fetch the package (automatically downloads and verifies .sig)
    let pkg_path = manager.fetch_package("my-app", "1.0.0", "/tmp").await?;

    // 5. Install the package
    manager.install_package(&pkg_path, "my-app", "1.0.0").await?;
    println!("Package installed successfully.");

    // 6. Uninstall the package later
    manager.uninstall_package("my-app", "1.0.0").await?;
    println!("Package uninstalled successfully.");

    Ok(())
}
```

### Multiple Trusted Keys

ETPM supports multiple trusted keys. A package is considered valid if its signature matches **any** of the registered keys. This is useful for supporting packages from multiple authors:

```rust
manager.add_trusted_key("PUBLIC_KEY_AUTHOR_1==")?;
manager.add_trusted_key("PUBLIC_KEY_AUTHOR_2==")?;
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
The `url` field is relative to the repository URL. ETPM automatically joins the repository URL with this field to form the final download link. The corresponding `.sig` file is expected at `{url}.sig`.

## Building from Source

To build the Rust library and the C dynamic library (`cdylib`):
```bash
cargo build --release
```
The compiled library will be located in `target/release/` (e.g., `libetpm.so`, `libetpm.dylib`, or `etpm.dll`).

## License

ETPM is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
