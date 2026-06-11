use ed25519_dalek::VerifyingKey;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tar::Archive;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::fetcher::Index;
use crate::unpack::{is_path_safe, unpack_package};

mod error;
mod fetcher;
mod ffi;
mod unpack;

pub use error::TpmError;
pub use unpack::ConflictStrategy;

pub struct PackageManager {
    repositories: Vec<Url>,
    root: PathBuf,
    packages: PathBuf,
    trusted_keys: Vec<VerifyingKey>,
    allow_unsigned: bool,
    index_cache: Arc<Mutex<HashMap<Url, Index>>>,
}

impl PackageManager {
    /// Creates a new instance of the manager.
    /// By default, the root directory is set to the current directory (".").
    /// By default, the packages directory is set to "./packages".
    pub fn new() -> Self {
        info!("Creating new PackageManager instance");
        Self {
            repositories: Vec::new(),
            root: PathBuf::from("."),
            packages: PathBuf::from("./packages"),
            trusted_keys: Vec::new(),
            allow_unsigned: false,
            index_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Explicitly sets or changes the root directory for package installation.
    /// If the directory does not exist, it will be created.
    pub fn set_root(&mut self, root: impl Into<PathBuf>) -> Result<(), TpmError> {
        let new_root = root.into();
        info!("Setting root directory to {}", new_root.display());
        if !new_root.exists() {
            debug!("Directory does not exist, creating...");
            std::fs::create_dir_all(&new_root).map_err(TpmError::Io)?;
        }
        self.root = new_root;
        Ok(())
    }

    /// Explicitly sets or changes the packages directory for storing package metadata.
    /// If the directory does not exist, it will be created.
    pub fn set_packages(&mut self, packages: impl Into<PathBuf>) -> Result<(), TpmError> {
        let new_packages = packages.into();
        info!("Setting packages directory to {}", new_packages.display());
        if !new_packages.exists() {
            debug!("Directory does not exist, creating...");
            std::fs::create_dir_all(&new_packages).map_err(TpmError::Io)?;
        }
        self.packages = new_packages;
        Ok(())
    }

    /// Adds a trusted Ed25519 public key (Base64 encoded) for signature verification.
    pub fn add_trusted_key(&mut self, public_key_base64: &str) -> Result<(), TpmError> {
        info!("Adding trusted key: {}", public_key_base64);
        let key_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            public_key_base64,
        )
        .map_err(|e| {
            error!("Invalid key format: {}", e);
            TpmError::InvalidSignature
        })?; // Или можно сделать отдельную ошибку InvalidKeyFormat

        let verifying_key = VerifyingKey::try_from(key_bytes.as_slice()).map_err(|e| {
            error!("Invalid key format: {}", e);
            TpmError::InvalidSignature
        })?;

        self.trusted_keys.push(verifying_key);
        debug!(
            "Trusted key added: {}, total: {}",
            public_key_base64,
            self.trusted_keys.len()
        );
        Ok(())
    }

    /// Sets whether unsigned packages are allowed.
    /// WARNING: This should only be set to `true` if you trust the repository and know the packages are signed.
    pub fn set_allow_unsigned(&mut self, allow: bool) {
        if allow {
            debug!("Allow unsigned packages: {}", allow);
        } else {
            debug!("Disallow unsigned packages: {}", allow);
        }
        self.allow_unsigned = allow;
    }

    /// Adds a new repository to the list.
    pub fn add_repository(&mut self, repo: impl AsRef<str>) -> Result<(), TpmError> {
        let mut url_str = repo.as_ref().to_string();
        if !url_str.ends_with('/') {
            url_str.push('/');
        }

        let url = Url::parse(&url_str).map_err(|e| {
            error!("Invalid repository URL: {}", e);
            TpmError::Repository(format!("Invalid repository URL: {}", e))
        })?;

        // Avoid duplicates
        if !self.repositories.contains(&url) {
            info!("Adding repository: {}", url);
            self.repositories.push(url);
        } else {
            info!("Repository already exists: {}", url);
        }
        Ok(())
    }

    /// A convenient constructor when you want to create a manager with a single repository.
    /// The root will remain the default ("."); it can be changed later via `set_root`.
    /// The packages will remain the default ("./packages"); it can be changed later via `set_packages`.
    pub fn with_repository(repo: impl AsRef<str>) -> Result<Self, TpmError> {
        info!("Creating manager with repository: {}", repo.as_ref());
        let mut manager = Self::new();
        manager.add_repository(repo)?;
        Ok(manager)
    }

    pub fn list_repositories(&self) -> &[Url] {
        &self.repositories
    }

    async fn get_index(&self, repo: &Url) -> Result<Index, TpmError> {
        let mut cache = self.index_cache.lock().await;
        if let Some(index) = cache.get(repo) {
            debug!("Cache hit: {}", repo);
            return Ok(index.clone());
        }

        let index = fetcher::fetch_index(repo).await?;
        cache.insert(repo.clone(), index.clone());
        debug!("Cache miss: {}", repo);
        Ok(index)
    }

    pub async fn fetch_package(
        &self,
        package_name: &str,
        version: &str,
        dest: impl AsRef<Path>,
    ) -> Result<PathBuf, TpmError> {
        info!("Fetching package: {}@{}", package_name, version);
        let dest = dest.as_ref();
        for repo in &self.repositories {
            let index = self.get_index(repo).await?;
            if fetcher::package_exists(&index, package_name, version) {
                info!("Found package: {}@{} in {}", package_name, version, repo);
                let package = index
                    .packages
                    .get(package_name)
                    .and_then(|versions| versions.iter().find(|v| v.version == version))
                    .unwrap();

                return fetcher::download_package(
                    repo,
                    package,
                    package_name,
                    dest,
                    &self.trusted_keys,
                    self.allow_unsigned,
                )
                .await;
            }
        }
        error!("Package not found: {}@{}", package_name, version);
        Err(TpmError::PackageNotFound(
            package_name.to_string(),
            version.to_string(),
        ))
    }

    /// Installs a package from the archive.
    ///
    /// # Arguments
    /// * `path_package` - Path to the .tp archive file
    /// * `package_name` - Name of the package
    /// * `package_version` - Version of the package
    pub async fn install_package(
        &self,
        path_package: impl AsRef<Path>,
        package_name: &str,
        package_version: &str,
    ) -> Result<(), TpmError> {
        info!("Installing package: {}@{}", package_name, package_version);
        let path = path_package.as_ref().to_path_buf();
        let root = self.root.clone();
        let packages = self.packages.clone();
        let name = package_name.to_string();
        let version = package_version.to_string();

        tokio::task::spawn_blocking(move || {
            // Create package directory: $packages/<name>-<ver>/
            let package_dir = packages.join(format!("{}-{}", name, version));
            std::fs::create_dir_all(&package_dir)?;
            info!("Created package directory: {}", package_dir.display());

            // Unpack overlay to root, addition to package_dir
            let filelist = unpack_package(&path, &root, &package_dir, ConflictStrategy::Overwrite)?;
            info!("Unpacked package to: {}", package_dir.display());

            // Generate filelist
            let filelist_path = package_dir.join("filelist");
            info!("Generating filelist: {}", filelist_path.display());
            let filelist_content: String = filelist
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("\n");
            info!("Filelist content: {}", filelist_content);
            std::fs::write(&filelist_path, filelist_content)?;

            Ok(())
        })
        .await
        .map_err(|e| TpmError::Repository(format!("Blocking task panicked: {}", e)))?
    }

    /// Uninstalls a package by removing all files listed in its filelist and the package directory.
    ///
    /// # Arguments
    /// * `package_name` - Name of the package to uninstall
    /// * `package_version` - Version of the package to uninstall
    ///
    /// # Behavior
    /// 1. Reads `$packages/<name>-<ver>/filelist`
    /// 2. Removes each file listed in the filelist from `$root`
    /// 3. Removes the entire `$packages/<name>-<ver>/` directory
    ///
    /// # Notes
    /// - Files that no longer exist are silently skipped
    /// - Parent directories are NOT removed (they may contain files from other packages)
    /// - Returns an error if the package directory does not exist
    pub async fn uninstall_package(
        &self,
        package_name: &str,
        package_version: &str,
    ) -> Result<(), TpmError> {
        info!("Uninstalling package: {}@{}", package_name, package_version);
        let root = self.root.clone();
        let packages = self.packages.clone();
        let name = package_name.to_string();
        let version = package_version.to_string();

        tokio::task::spawn_blocking(move || {
            let package_dir = packages.join(format!("{}-{}", name, version));
            let filelist_path = package_dir.join("filelist");

            if !package_dir.exists() {
                error!(
                    "Package directory does not exist: {}",
                    package_dir.display()
                );
                return Err(TpmError::PackageNotFound(name, version));
            }

            if filelist_path.exists() {
                info!("Reading filelist: {}", filelist_path.display());
                let content = std::fs::read_to_string(&filelist_path)?;
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    let file_path = PathBuf::from(line);

                    if !is_path_safe(&file_path) {
                        warn!("Skipping unsafe path in filelist: {}", file_path.display());
                        continue;
                    }

                    let full_path = root.join(&file_path);

                    if full_path.exists() {
                        if full_path.is_file() {
                            if let Err(e) = std::fs::remove_file(&full_path) {
                                warn!("Failed to remove {}: {}", full_path.display(), e);
                            }
                        } else {
                            warn!("{} is not a file, skipping", full_path.display());
                        }
                    }
                }
            }

            info!("Remove package directory: {}", package_dir.display());
            std::fs::remove_dir_all(&package_dir)?;

            info!("Package {} uninstalled", package_dir.display());
            Ok(())
        })
        .await
        .map_err(|e| {
            error!("Blocking task panicked: {}", e);
            TpmError::Repository(format!("Blocking task panicked: {}", e))
        })?
    }

    /// Reads a file from the `addition` directory of a downloaded `.tp` archive.
    /// This allows inspecting metadata or configuration files before installation.
    ///
    /// # Arguments
    /// * `package_path` - Path to the downloaded `.tp` archive.
    /// * `file_path` - Relative path to the file inside the `addition` directory
    ///   (e.g., `"manifest.json"` or `"config/settings.toml"`).
    ///
    /// # Returns
    /// A `Vec<u8>` containing the raw file contents. The caller is responsible for
    /// deserializing or parsing the data.
    pub fn read_addition_file(
        &self,
        package_path: impl AsRef<Path>,
        file_path: impl AsRef<Path>,
    ) -> Result<Vec<u8>, TpmError> {
        let req_path = file_path.as_ref();

        if !is_path_safe(req_path) || req_path.is_absolute() {
            return Err(TpmError::InvalidAdditionPath);
        }

        let path_str = req_path.to_string_lossy().replace('\\', "/");
        let target_prefix = format!("package/addition/{}", path_str);

        let file = std::fs::File::open(package_path.as_ref()).map_err(TpmError::Io)?;
        let decompressor = GzDecoder::new(file);
        let mut archive = Archive::new(decompressor);

        for entry in archive.entries().map_err(TpmError::Io)? {
            let mut entry = entry.map_err(TpmError::Io)?;
            let entry_path = entry.path().map_err(TpmError::Io)?;
            let entry_path_str = entry_path.to_string_lossy().replace('\\', "/");

            if entry_path_str == target_prefix {
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents).map_err(TpmError::Io)?;
                return Ok(contents);
            }
        }

        Err(TpmError::AdditionFileNotFound(path_str))
    }

    pub async fn check_update(
        &self,
        package_name: &str,
        current_version: &str,
    ) -> Result<bool, TpmError> {
        info!(
            "Checking updates for {} current {}",
            package_name, current_version
        );
        for repo in &self.repositories {
            debug!("Checking repository {} for updates", repo);
            let index = self.get_index(repo).await?;
            if fetcher::check_update(&index, package_name, current_version)? {
                info!(
                    "Update available for {}@{} in {}",
                    package_name, current_version, repo
                );
                return Ok(true);
            }
        }
        info!("No updates found for {}@{}", package_name, current_version);
        Ok(false)
    }

    pub async fn get_latest_version(&self, package_name: &str) -> Result<Option<String>, TpmError> {
        info!("Retrieving latest version for {}", package_name);
        let mut latest_overall: Option<semver::Version> = None;
        let mut latest_str: Option<String> = None;

        for repo in &self.repositories {
            debug!("Checking repository {} for latest version", repo);
            let index = self.get_index(repo).await?;
            if let Some(ver_str) = fetcher::get_latest_version_from_index(&index, package_name) {
                debug!("Found candidate version {} in {}", ver_str, repo);
                if let Ok(parsed) = semver::Version::parse(&ver_str) {
                    if latest_overall.as_ref().map_or(true, |max| &parsed > max) {
                        latest_overall = Some(parsed);
                        latest_str = Some(ver_str.clone());
                        info!(
                            "New latest version for {}: {} (from {})",
                            package_name, ver_str, repo
                        );
                    }
                }
            }
        }
        if let Some(ref v) = latest_str {
            info!("Latest version for {} is {}", package_name, v);
        } else {
            info!("No versions found for {}", package_name);
        }
        Ok(latest_str)
    }
}
