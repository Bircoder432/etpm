use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::fetcher::Index;
use crate::unpack::unpack_package;

mod error;
mod fetcher;
mod unpack;

pub use error::TpmError;
pub use unpack::ConflictStrategy;

pub struct PackageManager {
    repositories: Vec<Url>,
    root: PathBuf,
    index_cache: Arc<Mutex<HashMap<Url, Index>>>,
}

impl PackageManager {
    /// Creates a new instance of the manager.
    /// By default, the root directory is set to the current directory (".").
    pub fn new() -> Self {
        Self {
            repositories: Vec::new(),
            root: PathBuf::from("."),
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tpm::PackageManager;
    ///
    /// let manager = PackageManager::with_repository("https://example.com/repo/").unwrap();
    /// ```
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

    pub async fn install_package(&self, path_package: impl AsRef<Path>) -> Result<(), TpmError> {
        let path = path_package.as_ref().to_path_buf();
        let root = self.root.clone();

        tokio::task::spawn_blocking(move || {
            unpack_package(&path, &root, ConflictStrategy::Overwrite)
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
