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

/// Unpacks a compressed tar archive to the destination directory.
///
/// # Arguments
/// * `package_path` - Path to the .tar.gz file to unpack
/// * `destination` - Target directory for extracted files
/// * `conflict_strategy` - How to handle existing files
pub fn unpack_package(
    package_path: &Path,
    destination: &Path,
    conflict_strategy: ConflictStrategy,
) -> Result<(), TpmError> {
    let file = std::fs::File::open(package_path)?;
    let decompressor = GzDecoder::new(file);
    let mut archive = Archive::new(decompressor);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let archive_path = entry.path()?.into_owned();

        let relative_path: PathBuf = archive_path.components().skip(1).collect();

        if relative_path.as_os_str().is_empty() {
            continue;
        }

        if !is_path_safe(&relative_path) {
            return Err(TpmError::PathTraversal);
        }

        let dest_path = destination.join(&relative_path);

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
                    let parent = dest_path.parent().unwrap_or(destination);

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

    Ok(())
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

        // Danangerous paths (Tar Slip)
        assert!(!is_path_safe(Path::new("../file.txt")));
        assert!(!is_path_safe(Path::new("dir/../../file.txt")));
        assert!(!is_path_safe(Path::new("/etc/passwd"))); // Абсолютный путь (RootDir)

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

        let mut header = Header::new_gnu();
        header.set_path("my_package/safe_file.txt").unwrap();
        header.set_size(9);
        header.set_cksum();
        builder.append(&header, b"hello tar" as &[u8]).unwrap();

        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let dest = temp_dir.path().join("dest");
        std::fs::create_dir(&dest).unwrap();

        let result = unpack_package(&tar_path, &dest, ConflictStrategy::Overwrite);

        if let Err(ref e) = result {
            eprintln!("Error of unpacking: {:?}", e);
        }

        assert!(result.is_ok(), "Unpacking valid archive should not error");
        assert!(
            dest.join("safe_file.txt").exists(),
            "File should be unpacked to root of dest"
        );

        let content = std::fs::read_to_string(dest.join("safe_file.txt")).unwrap();
        assert_eq!(content, "hello tar");
    }
}
