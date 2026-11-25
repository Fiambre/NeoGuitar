use neoguitar_win::plugin::*;

#[test]
fn parses_plugin_answers() {
    assert_eq!(parse_info(r#"{"protocol":1,"name":"gprotab","version":"0.1.0"}"#).unwrap(), "0.1.0");
    assert!(parse_info(r#"{"protocol":2,"version":"9"}"#).unwrap_err().contains("protocol 2"));

    let p = parse_search(r#"{"results":[{"id":"nirvana/polly","artist":"Nirvana","title":"Polly"},{"id":"x"}],"has_next":true}"#).unwrap();
    assert_eq!(p.hits, vec![Hit { id: "nirvana/polly".into(), artist: "Nirvana".into(), title: "Polly".into() }]);
    assert!(p.has_next);

    assert_eq!(parse_search(r#"{"error":"sitio caído"}"#).unwrap_err(), "sitio caído");
    assert!(parse_search("no es json").is_err());
    assert_eq!(parse_download(r#"{"path":"C:\\a\\b.gp5"}"#).unwrap(), std::path::PathBuf::from(r"C:\a\b.gp5"));
}

#[test]
fn picks_the_windows_asset_and_demands_a_checksum() {
    let sum = "a".repeat(64);
    let ok = format!(
        r#"{{"tag_name":"v0.1.0","assets":[
        {{"name":"gprotab-v0.1.0-x86_64-unknown-linux-gnu.tar.gz","browser_download_url":"https://github.com/Fiambre/GTabs-Downloader/releases/download/v0.1.0/l.tar.gz","digest":"sha256:{sum}"}},
        {{"name":"gprotab-v0.1.0-x86_64-pc-windows-msvc.zip","browser_download_url":"https://github.com/Fiambre/GTabs-Downloader/releases/download/v0.1.0/w.zip","digest":"sha256:{}"}}]}}"#,
        "B".repeat(64)
    );
    let (url, sha, ver) = pick_asset(&ok).unwrap();
    assert!(url.ends_with("/w.zip"));
    assert_eq!(sha, "b".repeat(64));
    assert_eq!(ver, "0.1.0");

    // Sin huella, o alojado en otro sitio: se rechaza.
    let no_sum = r#"{"tag_name":"v1","assets":[{"name":"g-x86_64-pc-windows-msvc.zip","browser_download_url":"https://github.com/Fiambre/GTabs-Downloader/releases/download/v1/w.zip"}]}"#;
    assert!(pick_asset(no_sum).is_err());
    let elsewhere = format!(r#"{{"tag_name":"v1","assets":[{{"name":"g-x86_64-pc-windows-msvc.zip","browser_download_url":"https://evil.example/w.zip","digest":"sha256:{sum}"}}]}}"#);
    assert!(pick_asset(&elsewhere).is_err());
}

#[test]
fn extracts_the_exe_from_a_zip() {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("gprotab-v0.1.0-x86_64-pc-windows-msvc/README.md", o).unwrap();
        z.write_all(b"hola").unwrap();
        z.start_file("gprotab-v0.1.0-x86_64-pc-windows-msvc/gprotab.exe", o).unwrap();
        z.write_all(b"MZ-fake").unwrap();
        z.finish().unwrap();
    }
    assert_eq!(extract_exe(&buf).unwrap(), b"MZ-fake");
    assert!(extract_exe(b"no zip").is_err());
}

/// Necesita red y un `gprotab.exe` compilado: GPROTAB_EXE=<ruta> cargo test -p neoguitar-win --test plugin -- --ignored
#[test]
#[ignore]
fn runs_the_real_plugin() {
    let exe = std::env::var("GPROTAB_EXE").expect("GPROTAB_EXE");
    let cfg = std::env::temp_dir().join("neoguitar-plugin-test");
    let _ = std::fs::remove_dir_all(&cfg);
    std::fs::create_dir_all(cfg.join("plugins")).unwrap();
    std::fs::copy(&exe, cfg.join("plugins").join(EXE_NAME)).unwrap();
    // Solo este test toca el entorno (por eso va ignorado y es el único que lo cambia).
    unsafe { std::env::set_var("NEOGUITAR_CONFIG_DIR", &cfg) };

    assert!(is_installed());
    assert_eq!(version().unwrap(), "0.1.0");
    let page = search("nirvana", 1).unwrap();
    assert!(!page.hits.is_empty());
    let out = cfg.join("downloads");
    let path = download(&page.hits[0].id, &out).unwrap();
    assert!(path.is_file() && std::fs::metadata(&path).unwrap().len() > 1000);
    // El archivo bajado lo entiende el lector de NeoGuitar.
    let song = neoguitar_core::load(&path);
    assert!(song.is_ok(), "{:?}", song.err());
    uninstall().unwrap();
    assert!(!is_installed());
}
/// Necesita red: instala el complemento de verdad desde la última release y lo usa.
#[test]
#[ignore]
fn installs_from_github() {
    let cfg = std::env::temp_dir().join("neoguitar-plugin-install-test");
    let _ = std::fs::remove_dir_all(&cfg);
    unsafe { std::env::set_var("NEOGUITAR_CONFIG_DIR", &cfg) };
    assert!(!is_installed());
    let v = install().unwrap();
    assert!(is_installed(), "version {v}");
    assert_eq!(version().unwrap(), v);
    assert!(!search("polly", 1).unwrap().hits.is_empty());
    uninstall().unwrap();
}