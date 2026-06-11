use flate2::read::GzDecoder;
use std::path::{Path, PathBuf};
use tar::Archive;

use crate::error::TpmError;

/// Checks if the path is safe for unpacking.
/// Prevents Path Traversal (Tar Slip) attempts such as `..`, root directories `/`, or drive prefixes `C:`.
pub fn is_path_safe(path: &Path) -> bool {
    for component in path.components() {
        match component {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => return false,
        }
    }
    true
}

/// Strategy to handle file conflicts during unpacking.
pub enum ConflictStrategy {
    Overwrite,
    Skip,
    Rename,
}

/// Unpacks overlay and addition directories from a package archive.
///
/// # Arguments
/// * `package_path` - Path to the .tar.gz file to unpack
/// * `overlay_dest` - Target directory for overlay files (usually root)
/// * `addition_dest` - Target directory for addition files (usually packages/name-ver)
/// * `conflict_strategy` - How to handle existing files
///
/// # Returns
/// List of relative paths from overlay directory (for filelist generation)
pub fn unpack_package(
    package_path: &Path,
    overlay_dest: &Path,
    addition_dest: &Path,
    conflict_strategy: ConflictStrategy,
) -> Result<Vec<PathBuf>, TpmError> {
    let file = std::fs::File::open(package_path)?;
    let decompressor = GzDecoder::new(file);
    let mut archive = Archive::new(decompressor);

    let mut filelist = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let archive_path = entry.path()?.into_owned();

        // Archive structure: package/overlay/... or package/addition/...
        // Skip the root directory "package/"
        let components: Vec<_> = archive_path.components().collect();
        if components.len() < 2 {
            continue; // This is the root directory itself
        }

        // Skip "package/"
        let relative_path: PathBuf = components.into_iter().skip(1).collect();

        // Determine if this is overlay or addition
        let first_component = relative_path.components().next();
        if first_component.is_none() {
            continue;
        }

        let first_str = first_component.unwrap().as_os_str().to_str().unwrap_or("");

        if first_str == "overlay" {
            // Unpack to overlay_dest
            let inner_path: PathBuf = relative_path.components().skip(1).collect();
            if inner_path.as_os_str().is_empty() {
                continue;
            }

            if !is_path_safe(&inner_path) {
                return Err(TpmError::PathTraversal);
            }

            let dest_path = overlay_dest.join(&inner_path);

            if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(&dest_path)?;
                continue;
            }

            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Handle conflicts
            if dest_path.exists() {
                match conflict_strategy {
                    ConflictStrategy::Overwrite => {
                        if dest_path.is_file() {
                            std::fs::remove_file(&dest_path)?;
                        }
                    }
                    ConflictStrategy::Skip => continue,
                    ConflictStrategy::Rename => {
                        let stem = dest_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("file");
                        let ext = dest_path.extension().and_then(|s| s.to_str());
                        let parent = dest_path.parent().unwrap_or(overlay_dest);

                        let mut counter = 1;
                        loop {
                            let candidate = match ext {
                                Some(ext) => parent.join(format!("{}_{}.{}", stem, counter, ext)),
                                None => parent.join(format!("{}_{}", stem, counter)),
                            };

                            if !candidate.exists() {
                                entry.unpack(&candidate)?;
                                break;
                            }
                            counter += 1;
                        }
                        continue;
                    }
                }
            }

            entry.unpack(&dest_path)?;
            filelist.push(inner_path);
        } else if first_str == "addition" {
            // Unpack to addition_dest
            let inner_path: PathBuf = relative_path.components().skip(1).collect();
            if inner_path.as_os_str().is_empty() {
                continue;
            }

            if !is_path_safe(&inner_path) {
                return Err(TpmError::PathTraversal);
            }

            let dest_path = addition_dest.join(&inner_path);

            if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(&dest_path)?;
                continue;
            }

            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if dest_path.exists() {
                match conflict_strategy {
                    ConflictStrategy::Overwrite => {
                        if dest_path.is_file() {
                            std::fs::remove_file(&dest_path)?;
                        }
                    }
                    ConflictStrategy::Skip => continue,
                    ConflictStrategy::Rename => {
                        let stem = dest_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("file");
                        let ext = dest_path.extension().and_then(|s| s.to_str());
                        let parent = dest_path.parent().unwrap_or(addition_dest);

                        let mut counter = 1;
                        loop {
                            let candidate = match ext {
                                Some(ext) => parent.join(format!("{}_{}.{}", stem, counter, ext)),
                                None => parent.join(format!("{}_{}", stem, counter)),
                            };

                            if !candidate.exists() {
                                entry.unpack(&candidate)?;
                                break;
                            }
                            counter += 1;
                        }
                        continue;
                    }
                }
            }

            entry.unpack(&dest_path)?;
        }
    }

    Ok(filelist)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    #[test]
    fn test_path_safety_validation() {
        // Safe paths
        assert!(is_path_safe(Path::new("file.txt")));
        assert!(is_path_safe(Path::new("dir/file.txt")));
        assert!(is_path_safe(Path::new("./file.txt")));
        assert!(is_path_safe(Path::new("a/b/c/file.txt")));

        // Dangerous paths (Tar Slip)
        assert!(!is_path_safe(Path::new("../file.txt")));
        assert!(!is_path_safe(Path::new("dir/../../file.txt")));
        assert!(!is_path_safe(Path::new("/etc/passwd")));

        // Windows-specific path prefixes are rejected
        #[cfg(windows)]
        assert!(!is_path_safe(Path::new("C:/windows/file.txt")));
    }

    #[test]
    fn test_valid_archive_unpacking() {
        let temp_dir = tempfile::tempdir().unwrap();
        let tar_path = temp_dir.path().join("valid_package.tar.gz");

        let file = std::fs::File::create(&tar_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        // Create overlay file
        let mut header = Header::new_gnu();
        header.set_path("package/overlay/safe_file.txt").unwrap();
        header.set_size(9);
        header.set_cksum();
        builder.append(&header, b"hello tar" as &[u8]).unwrap();

        // Create addition file
        let mut header = Header::new_gnu();
        header.set_path("package/addition/meta.txt").unwrap();
        header.set_size(4);
        header.set_cksum();
        builder.append(&header, b"meta" as &[u8]).unwrap();

        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let overlay_dest = temp_dir.path().join("overlay");
        std::fs::create_dir(&overlay_dest).unwrap();

        let addition_dest = temp_dir.path().join("addition");
        std::fs::create_dir(&addition_dest).unwrap();

        let result = unpack_package(
            &tar_path,
            &overlay_dest,
            &addition_dest,
            ConflictStrategy::Overwrite,
        );

        if let Err(ref e) = result {
            eprintln!("Error of unpacking: {:?}", e);
        }

        assert!(result.is_ok(), "Unpacking valid archive should not error");

        let filelist = result.unwrap();
        assert_eq!(filelist.len(), 1);
        assert_eq!(filelist[0], PathBuf::from("safe_file.txt"));

        assert!(
            overlay_dest.join("safe_file.txt").exists(),
            "Overlay file should be unpacked"
        );
        assert!(
            addition_dest.join("meta.txt").exists(),
            "Addition file should be unpacked"
        );

        let content = std::fs::read_to_string(overlay_dest.join("safe_file.txt")).unwrap();
        assert_eq!(content, "hello tar");

        let content = std::fs::read_to_string(addition_dest.join("meta.txt")).unwrap();
        assert_eq!(content, "meta");
    }
}
