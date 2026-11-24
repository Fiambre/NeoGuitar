use neoguitar_win::update::*;

#[test]
fn version_comparison() {
    assert!(is_newer("0.2.0", "v0.3.0"));
    assert!(is_newer("0.2.0", "0.2.1"));
    assert!(is_newer("0.9.9", "1.0.0"));
    assert!(!is_newer("0.3.0", "0.3.0"));
    assert!(!is_newer("0.3.0", "0.2.9"));
    assert!(!is_newer("0.3.0", "garbage"));
    assert!(is_newer("0.2.0", "0.10.0"));
}

#[test]
fn parses_github_release() {
    let json = r#"{"tag_name":"v0.3.0","html_url":"https://github.com/Fiambre/NeoGuitar/releases/tag/v0.3.0","body":"notes",
      "assets":[
        {"name":"NeoGuitar-0.3.0-portable.zip","size":10,"digest":"sha256:aa","browser_download_url":"https://github.com/Fiambre/NeoGuitar/releases/download/v0.3.0/NeoGuitar-0.3.0-portable.zip"},
        {"name":"NeoGuitar-0.3.0-setup.exe","size":123,"digest":"sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789","browser_download_url":"https://github.com/Fiambre/NeoGuitar/releases/download/v0.3.0/NeoGuitar-0.3.0-setup.exe"}]}"#;
    let r = parse_release(json).unwrap();
    assert_eq!(r.version, "0.3.0");
    let i = r.installer.unwrap();
    assert!(i.url.ends_with("setup.exe"));
    assert_eq!(i.size, 123);
    assert_eq!(i.sha256, "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789");

    // Without a digest there is nothing to verify, so no installer is offered.
    let r = parse_release(r#"{"tag_name":"v1.0.0","assets":[{"name":"x-setup.exe","browser_download_url":"u","size":1}]}"#).unwrap();
    assert!(r.installer.is_none());
}

#[test]
fn sha256_known_value() {
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn download_refuses_foreign_hosts_and_missing_checksum() {
    let dest = std::env::temp_dir().join("neoguitar-update-test.exe");
    let foreign = Installer { url: "https://example.com/NeoGuitar-9-setup.exe".into(), size: 1, sha256: "0".repeat(64) };
    assert!(download(&foreign, &dest).is_err());
    let nosum = Installer { url: "https://github.com/Fiambre/NeoGuitar/releases/download/v9/x-setup.exe".into(), size: 1, sha256: String::new() };
    assert!(download(&nosum, &dest).is_err());
    assert!(!dest.exists());
}

#[test]
fn picks_newest_windows_release_among_platforms() {
    let json = r#"[
      {"tag_name":"android-v9.0.0","assets":[]},
      {"tag_name":"win-v0.4.0","draft":true,"assets":[]},
      {"tag_name":"win-v0.5.0-beta","prerelease":true,"assets":[]},
      {"tag_name":"win-v0.4.0","assets":[{"name":"NeoGuitar-0.4.0-setup.exe","size":5,"digest":"sha256:aa","browser_download_url":"https://github.com/Fiambre/NeoGuitar/releases/download/win-v0.4.0/NeoGuitar-0.4.0-setup.exe"}]},
      {"tag_name":"v0.3.0","assets":[]},
      {"tag_name":"nightly","assets":[]}]"#;
    // The draft and the beta are skipped; the Android tag is ignored although its number is higher.
    let r = parse_windows_releases(json).unwrap().unwrap();
    assert_eq!(r.version, "0.4.0");
    assert!(r.installer.is_some());
    // Legacy tags (before the split by platform) still count.
    let r = parse_windows_releases(r#"[{"tag_name":"android-v1.0.0"},{"tag_name":"v0.3.0","assets":[]}]"#).unwrap().unwrap();
    assert_eq!(r.version, "0.3.0");
    assert!(parse_windows_releases(r#"[{"tag_name":"android-v1.0.0"}]"#).unwrap().is_none());
}

/// Needs internet: the published release must verify against its own checksum.
#[test]
#[ignore]
fn latest_release_checksum_matches() {
    let json = ureq::get("https://api.github.com/repos/Fiambre/NeoGuitar/releases?per_page=30").header("User-Agent", "NeoGuitar-test").call().unwrap().body_mut().read_to_string().unwrap();
    let r = parse_windows_releases(&json).unwrap().expect("a Windows release");
    let installer = r.installer.expect("installer asset with digest");
    let dest = installer_path("test-download");
    download(&installer, &dest).expect("download and verify");
    assert_eq!(std::fs::metadata(&dest).unwrap().len(), installer.size);
    let _ = std::fs::remove_file(dest);
}

/// Needs internet: with an old simulated version the latest release is reported as an update.
#[test]
#[ignore]
fn check_reports_update_for_old_version() {
    unsafe { std::env::set_var("NEOGUITAR_CURRENT_VERSION", "0.1.0") };
    let r = check().expect("request").expect("an update");
    assert!(is_newer("0.1.0", &r.version));
}
