use futures_util::StreamExt;
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::error::TpmError;

#[derive(Debug, Deserialize, Clone)]
pub struct PackageVersion {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Index {
    pub packages: BTreeMap<String, Vec<PackageVersion>>,
}

pub async fn fetch_index(repository: &Url) -> Result<Index, TpmError> {
    // Url::join correctly joins paths if repository ends with '/'
    let url = repository.join("index.ron")?;
    let text = reqwest::get(url).await?.text().await?;
    Ok(ron::from_str(&text)?)
}

pub fn package_exists(index: &Index, package_name: &str, version: &str) -> bool {
    index
        .packages
        .get(package_name)
        .map(|versions| versions.iter().any(|v| v.version == version))
        .unwrap_or(false)
}

pub fn check_update(
    index: &Index,
    package_name: &str,
    current_version: &str,
) -> Result<bool, TpmError> {
    let versions = index.packages.get(package_name).ok_or_else(|| {
        TpmError::PackageNotFound(package_name.to_string(), current_version.to_string())
    })?;

    let current = Version::parse(current_version)?;
    let latest = versions
        .iter()
        .filter_map(|v| Version::parse(&v.version).ok())
        .max()
        .ok_or_else(|| TpmError::Repository("No valid versions found".into()))?;

    Ok(latest > current)
}

pub async fn download_package(
    repository: &Url,
    package: &PackageVersion,
    package_name: &str,
    destination: &Path,
) -> Result<PathBuf, TpmError> {
    let url = repository.join(&package.url)?;
    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        return Err(TpmError::Repository(format!(
            "Failed to download: {}",
            response.status()
        )));
    }

    // Atomic download: write to temp file to avoid corruption on network failure
    let temp_path = destination.join(format!("{}-{}.tp.tmp", package_name, package.version));
    let final_path = destination.join(format!("{}-{}.tp", package_name, package.version));

    let mut file = tokio::fs::File::create(&temp_path).await?;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item?;
        file.write_all(&chunk).await?;
    }

    file.flush().await?;
    drop(file);

    tokio::fs::rename(&temp_path, &final_path).await?;

    Ok(final_path)
}

pub fn get_latest_version_from_index(index: &Index, package_name: &str) -> Option<String> {
    index
        .packages
        .get(package_name)?
        .iter()
        .filter_map(|v| {
            Version::parse(&v.version)
                .ok()
                .map(|sv| (sv, v.version.clone()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, ver)| ver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_parsing() {
        let ron_str = r#"(packages: {"my-pkg": [(version: "1.0.0", url: "1.tp")]})"#;
        let index: Index = ron::from_str(ron_str).unwrap();
        assert_eq!(index.packages.len(), 1);
    }

    #[test]
    fn test_latest_version_logic() {
        let ron_str = r#"(packages: {"pkg": [(version: "1.0.0", url: "1"), (version: "2.0.0", url: "2"), (version: "1.5.0", url: "3")]})"#;
        let index: Index = ron::from_str(ron_str).unwrap();
        assert_eq!(
            get_latest_version_from_index(&index, "pkg").unwrap(),
            "2.0.0"
        );
    }
}
