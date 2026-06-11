use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer, SigningKey};
use etpm::{PackageManager, TpmError};
use rand_core::OsRng;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INDEX_RON: &str =
    r#"(packages: {"test-pkg": [(version: "1.0.0", url: "test-pkg-1.0.0.tp")]})"#;

#[tokio::test]
async fn test_fetch_with_valid_signature() {
    let server = MockServer::start().await;
    let dir = tempdir().unwrap();

    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    let pub_key_b64 = STANDARD.encode(verifying_key.to_bytes());

    let pkg_content = b"verified package payload";
    let signature: Signature = signing_key.sign(pkg_content);
    let sig_b64 = STANDARD.encode(signature.to_bytes());

    Mock::given(method("GET"))
        .and(path("/index.ron"))
        .respond_with(ResponseTemplate::new(200).set_body_string(INDEX_RON))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-pkg-1.0.0.tp"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(pkg_content))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-pkg-1.0.0.tp.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sig_b64))
        .mount(&server)
        .await;

    let mut manager = PackageManager::new();
    manager.set_root(dir.path()).unwrap();
    manager.set_packages(dir.path().join("packages")).unwrap();
    manager.add_repository(server.uri()).unwrap();
    manager.add_trusted_key(&pub_key_b64).unwrap();

    let result = manager.fetch_package("test-pkg", "1.0.0", dir.path()).await;
    assert!(
        result.is_ok(),
        "Should successfully download the package with a valid signature"
    );

    let downloaded_path = result.unwrap();
    assert_eq!(std::fs::read(&downloaded_path).unwrap(), pkg_content);
}

#[tokio::test]
async fn test_fetch_with_tampered_content() {
    let server = MockServer::start().await;
    let dir = tempdir().unwrap();

    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    let pub_key_b64 = STANDARD.encode(verifying_key.to_bytes());

    let original_content = b"original content";
    let signature: Signature = signing_key.sign(original_content);
    let sig_b64 = STANDARD.encode(signature.to_bytes());

    Mock::given(method("GET"))
        .and(path("/index.ron"))
        .respond_with(ResponseTemplate::new(200).set_body_string(INDEX_RON))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-pkg-1.0.0.tp"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"TAMPERED CONTENT"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-pkg-1.0.0.tp.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sig_b64))
        .mount(&server)
        .await;

    let mut manager = PackageManager::new();
    manager.set_root(dir.path()).unwrap();
    manager.set_packages(dir.path().join("packages")).unwrap();
    manager.add_repository(server.uri()).unwrap();
    manager.add_trusted_key(&pub_key_b64).unwrap();

    let result = manager.fetch_package("test-pkg", "1.0.0", dir.path()).await;
    assert!(result.is_err(), "Should fail when the content is tampered");
    assert!(matches!(result.unwrap_err(), TpmError::InvalidSignature));

    assert!(!dir.path().join("test-pkg-1.0.0.tp.tmp").exists());
}

#[tokio::test]
async fn test_fetch_without_trusted_keys() {
    let server = MockServer::start().await;
    let dir = tempdir().unwrap();

    Mock::given(method("GET"))
        .and(path("/index.ron"))
        .respond_with(ResponseTemplate::new(200).set_body_string(INDEX_RON))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-pkg-1.0.0.tp"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"some data"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-pkg-1.0.0.tp.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_string("c2ln"))
        .mount(&server)
        .await;

    let mut manager = PackageManager::new();
    manager.set_root(dir.path()).unwrap();
    manager.set_packages(dir.path().join("packages")).unwrap();
    manager.add_repository(server.uri()).unwrap();

    let result = manager.fetch_package("test-pkg", "1.0.0", dir.path()).await;
    assert!(result.is_err(), "Should fail if there are no trusted keys");
}
