use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::StreamExt;
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::error::TpmError;
use tracing::{debug, error, info, warn};

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
    debug!("Fetching index from {}", url);
    let response = reqwest::get(url).await?;
    if !response.status().is_success() {
        error!("Failed to fetch index: {}", response.status());
        return Err(TpmError::Repository(format!(
            "Failed to fetch index: {}",
            response.status()
        )));
    }
    let text = response.text().await?;
    let index = ron::from_str(&text)?;
    debug!("Index parsed for repository {}", repository);
    Ok(index)
}

pub fn package_exists(index: &Index, package_name: &str, version: &str) -> bool {
    debug!("Checking existence of package {}@{}", package_name, version);
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
    debug!(
        "Checking updates for {} current {}",
        package_name, current_version
    );
    let versions = index.packages.get(package_name).ok_or_else(|| {
        TpmError::PackageNotFound(package_name.to_string(), current_version.to_string())
    })?;

    let current = Version::parse(current_version)?;
    let latest = versions
        .iter()
        .filter_map(|v| Version::parse(&v.version).ok())
        .max()
        .ok_or_else(|| TpmError::Repository("No valid versions found".into()))?;

    debug!("Latest version found: {}", latest);
    Ok(latest > current)
}

pub async fn download_package(
    repository: &Url,
    package: &PackageVersion,
    package_name: &str,
    destination: &Path,
    trusted_keys: &[VerifyingKey],
    allow_unsigned: bool,
) -> Result<PathBuf, TpmError> {
    let pkg_url = repository.join(&package.url)?;
    let sig_url = repository.join(&format!("{}.sig", package.url))?;

    info!(
        "Downloading package {}@{} from {}",
        package_name, package.version, pkg_url
    );

    let temp_pkg_path = destination.join(format!("{}-{}.tp.tmp", package_name, package.version));
    let final_pkg_path = destination.join(format!("{}-{}.tp", package_name, package.version));

    debug!(
        "Temp path: {} final path: {}",
        temp_pkg_path.display(),
        final_pkg_path.display()
    );

    let response = reqwest::get(pkg_url.clone()).await?;
    if !response.status().is_success() {
        error!(
            "Failed to download package {}: HTTP {}",
            pkg_url,
            response.status()
        );
        return Err(TpmError::Repository(format!(
            "Failed to download package: {}",
            response.status()
        )));
    }

    let mut file = tokio::fs::File::create(&temp_pkg_path).await?;
    let mut stream = response.bytes_stream();
    let mut file_bytes = Vec::new();
    let mut total: usize = 0;

    while let Some(item) = stream.next().await {
        let chunk = item?;
        total += chunk.len();
        file_bytes.extend_from_slice(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    debug!(
        "Downloaded {} bytes for {}@{}",
        total, package_name, package.version
    );

    if trusted_keys.is_empty() {
        if allow_unsigned {
            warn!(
                "Signature verification skipped (allow_unsigned mode) for {}@{}",
                package_name, package.version
            );
        } else {
            tokio::fs::remove_file(&temp_pkg_path).await?;
            error!("No trusted keys configured for signature verification");
            return Err(TpmError::Repository(
                "No trusted keys configured for signature verification".into(),
            ));
        }
    } else {
        let sig_response = reqwest::get(sig_url.clone()).await?;
        if !sig_response.status().is_success() {
            tokio::fs::remove_file(&temp_pkg_path).await?;
            error!(
                "Failed to download signature: HTTP {}",
                sig_response.status()
            );
            return Err(TpmError::InvalidSignature);
        }

        let sig_text = sig_response.text().await?;
        let sig_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, sig_text.trim())
                .map_err(|_| TpmError::InvalidSignature)?;

        let signature =
            Signature::try_from(sig_bytes.as_slice()).map_err(|_| TpmError::InvalidSignature)?;

        let mut signature_valid = false;
        for key in trusted_keys {
            if key.verify(&file_bytes, &signature).is_ok() {
                signature_valid = true;
                break;
            } else {
                debug!("Signature did not match this key");
            }
        }

        if !signature_valid {
            tokio::fs::remove_file(&temp_pkg_path).await?;
            error!(
                "Signature verification failed for {}@{}",
                package_name, package.version
            );
            return Err(TpmError::InvalidSignature);
        } else {
            info!(
                "Signature verified for {}@{}",
                package_name, package.version
            );
        }
    }

    tokio::fs::rename(&temp_pkg_path, &final_pkg_path).await?;
    info!("Saved package to {}", final_pkg_path.display());

    Ok(final_pkg_path)
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
