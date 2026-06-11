use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::fetcher::Index;
use crate::unpack::{is_path_safe, unpack_package};

mod error;
mod fetcher;
mod unpack;

pub use error::TpmError;
pub use unpack::ConflictStrategy;

pub struct PackageManager {
    repositories: Vec<Url>,
    root: PathBuf,
    packages: PathBuf,
    index_cache: Arc<Mutex<HashMap<Url, Index>>>,
}

impl PackageManager {
    /// Creates a new instance of the manager.
    /// By default, the root directory is set to the current directory (".").
    /// By default, the packages directory is set to "./packages".
    pub fn new() -> Self {
        Self {
            repositories: Vec::new(),
            root: PathBuf::from("."),
            packages: PathBuf::from("./packages"),
            index_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Explicitly sets or changes the root directory for package installation.
    /// If the directory does not exist, it will be created.
    pub fn set_root(&mut self, root: impl Into<PathBuf>) -> Result<(), TpmError> {
        let new_root = root.into();
        if !new_root.exists() {
            std::fs::create_dir_all(&new_root).map_err(TpmError::Io)?;
        }
        self.root = new_root;
        Ok(())
    }

    /// Explicitly sets or changes the packages directory for storing package metadata.
    /// If the directory does not exist, it will be created.
    pub fn set_packages(&mut self, packages: impl Into<PathBuf>) -> Result<(), TpmError> {
        let new_packages = packages.into();
        if !new_packages.exists() {
            std::fs::create_dir_all(&new_packages).map_err(TpmError::Io)?;
        }
        self.packages = new_packages;
        Ok(())
    }

    /// Adds a new repository to the list.
    pub fn add_repository(&mut self, repo: impl AsRef<str>) -> Result<(), TpmError> {
        let mut url_str = repo.as_ref().to_string();
        if !url_str.ends_with('/') {
            url_str.push('/');
        }

        let url = Url::parse(&url_str)
            .map_err(|e| TpmError::Repository(format!("Invalid repository URL: {}", e)))?;

        // Avoid duplicates
        if !self.repositories.contains(&url) {
            self.repositories.push(url);
        }
        Ok(())
    }

    /// A convenient constructor when you want to create a manager with a single repository.
    /// The root will remain the default ("."); it can be changed later via `set_root`.
    /// The packages will remain the default ("./packages"); it can be changed later via `set_packages`.
    pub fn with_repository(repo: impl AsRef<str>) -> Result<Self, TpmError> {
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
            return Ok(index.clone());
        }

        let index = fetcher::fetch_index(repo).await?;
        cache.insert(repo.clone(), index.clone());
        Ok(index)
    }

    pub async fn fetch_package(
        &self,
        package_name: &str,
        version: &str,
        dest: impl AsRef<Path>,
    ) -> Result<PathBuf, TpmError> {
        let dest = dest.as_ref();
        for repo in &self.repositories {
            let index = self.get_index(repo).await?;
            if fetcher::package_exists(&index, package_name, version) {
                let package = index
                    .packages
                    .get(package_name)
                    .and_then(|versions| versions.iter().find(|v| v.version == version))
                    .unwrap();

                return fetcher::download_package(repo, package, package_name, dest).await;
            }
        }
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
        let path = path_package.as_ref().to_path_buf();
        let root = self.root.clone();
        let packages = self.packages.clone();
        let name = package_name.to_string();
        let version = package_version.to_string();

        tokio::task::spawn_blocking(move || {
            // Create package directory: $packages/<name>-<ver>/
            let package_dir = packages.join(format!("{}-{}", name, version));
            std::fs::create_dir_all(&package_dir)?;

            // Unpack overlay to root, addition to package_dir
            let filelist = unpack_package(&path, &root, &package_dir, ConflictStrategy::Overwrite)?;

            // Generate filelist
            let filelist_path = package_dir.join("filelist");
            let filelist_content: String = filelist
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("\n");
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
        let root = self.root.clone();
        let packages = self.packages.clone();
        let name = package_name.to_string();
        let version = package_version.to_string();

        tokio::task::spawn_blocking(move || {
            let package_dir = packages.join(format!("{}-{}", name, version));
            let filelist_path = package_dir.join("filelist");

            if !package_dir.exists() {
                return Err(TpmError::PackageNotFound(name, version));
            }

            if filelist_path.exists() {
                let content = std::fs::read_to_string(&filelist_path)?;
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    let file_path = PathBuf::from(line);

                    if !is_path_safe(&file_path) {
                        eprintln!(
                            "Warning: skipping unsafe path in filelist: {}",
                            file_path.display()
                        );
                        continue;
                    }

                    let full_path = root.join(&file_path);

                    if full_path.exists() {
                        if full_path.is_file() {
                            if let Err(e) = std::fs::remove_file(&full_path) {
                                eprintln!(
                                    "Warning: failed to remove {}: {}",
                                    full_path.display(),
                                    e
                                );
                            }
                        } else {
                            eprintln!("Warning: {} is not a file, skipping", full_path.display());
                        }
                    }
                }
            }

            std::fs::remove_dir_all(&package_dir)?;

            Ok(())
        })
        .await
        .map_err(|e| TpmError::Repository(format!("Blocking task panicked: {}", e)))?
    }

    pub async fn check_update(
        &self,
        package_name: &str,
        current_version: &str,
    ) -> Result<bool, TpmError> {
        for repo in &self.repositories {
            let index = self.get_index(repo).await?;
            if fetcher::check_update(&index, package_name, current_version)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn get_latest_version(&self, package_name: &str) -> Result<Option<String>, TpmError> {
        let mut latest_overall: Option<semver::Version> = None;
        let mut latest_str: Option<String> = None;

        for repo in &self.repositories {
            let index = self.get_index(repo).await?;
            if let Some(ver_str) = fetcher::get_latest_version_from_index(&index, package_name) {
                if let Ok(parsed) = semver::Version::parse(&ver_str) {
                    if latest_overall.as_ref().map_or(true, |max| &parsed > max) {
                        latest_overall = Some(parsed);
                        latest_str = Some(ver_str);
                    }
                }
            }
        }
        Ok(latest_str)
    }
}
