//! Complemento opcional para buscar tabs en internet.
//!
//! NeoGuitar no incluye código de descarga: el complemento es un programa aparte (`gprotab.exe`, del proyecto
//! https://github.com/Fiambre/GTabs-Downloader) que el usuario instala si quiere. Se ejecuta como proceso y
//! habla JSON por stdout (ver el README de ese proyecto, protocolo versión 1).
//!
//! Se guarda en `<carpeta de ajustes>\plugins\gprotab.exe`. Al instalarlo se baja de las Releases de ese
//! repositorio y se verifica su SHA-256 antes de escribirlo.

use crate::settings::Settings;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const REPO: &str = "Fiambre/GTabs-Downloader";
const DOWNLOAD_PREFIX: &str = "https://github.com/Fiambre/GTabs-Downloader/releases/download/";
const USER_AGENT: &str = "NeoGuitar (https://github.com/Fiambre/NeoGuitar)";
const ASSET_SUFFIX: &str = "x86_64-pc-windows-msvc.zip";
pub const EXE_NAME: &str = "gprotab.exe";
/// Versión del protocolo que entiende esta versión de NeoGuitar.
pub const PROTOCOL: u64 = 1;

/// Un resultado de búsqueda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// `artista/cancion`; se le pasa tal cual a `download`.
    pub id: String,
    pub artist: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    pub hits: Vec<Hit>,
    pub has_next: bool,
}

pub fn dir() -> Option<PathBuf> {
    Settings::path().and_then(|p| p.parent().map(|d| d.join("plugins")))
}

pub fn exe_path() -> Option<PathBuf> {
    dir().map(|d| d.join(EXE_NAME))
}

pub fn is_installed() -> bool {
    exe_path().is_some_and(|p| p.is_file())
}

/// Borra el complemento instalado.
pub fn uninstall() -> Result<(), String> {
    match exe_path() {
        Some(p) if p.exists() => std::fs::remove_file(&p).map_err(|e| format!("could not remove {}: {e}", p.display())),
        _ => Ok(()),
    }
}

// ---- Interpretación de las respuestas (sin red, con pruebas) ----

fn parse_json(text: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("unexpected plugin output: {e}"))?;
    if let Some(err) = v["error"].as_str() {
        return Err(err.to_string());
    }
    Ok(v)
}

/// `plugin info` -> versión del complemento, comprobando que habla un protocolo que entendemos.
pub fn parse_info(text: &str) -> Result<String, String> {
    let v = parse_json(text)?;
    let proto = v["protocol"].as_u64().unwrap_or(0);
    if proto != PROTOCOL {
        return Err(format!("the plugin speaks protocol {proto}, this NeoGuitar needs {PROTOCOL}; update one of them"));
    }
    Ok(v["version"].as_str().unwrap_or("?").to_string())
}

pub fn parse_search(text: &str) -> Result<Page, String> {
    let v = parse_json(text)?;
    let hits = v["results"]
        .as_array()
        .ok_or("the plugin answer has no results")?
        .iter()
        .filter_map(|r| {
            Some(Hit { id: r["id"].as_str()?.to_string(), artist: r["artist"].as_str()?.to_string(), title: r["title"].as_str()?.to_string() })
        })
        .collect();
    Ok(Page { hits, has_next: v["has_next"].as_bool().unwrap_or(false) })
}

pub fn parse_download(text: &str) -> Result<PathBuf, String> {
    let v = parse_json(text)?;
    v["path"].as_str().map(PathBuf::from).ok_or_else(|| "the plugin answer has no file path".to_string())
}

/// Del JSON de un release de GitHub: (url del zip, huella SHA-256, versión).
pub fn pick_asset(release_json: &str) -> Result<(String, String, String), String> {
    let v: Value = serde_json::from_str(release_json).map_err(|e| format!("bad release data: {e}"))?;
    let tag = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
    let assets = v["assets"].as_array().ok_or("the release has no files")?;
    let zip = assets
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(ASSET_SUFFIX)))
        .ok_or("the release has no Windows build of the plugin")?;
    let url = zip["browser_download_url"].as_str().ok_or("the release file has no URL")?.to_string();
    if !url.starts_with(DOWNLOAD_PREFIX) {
        return Err("the plugin is not hosted in the project's releases".into());
    }
    let sha = zip["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(str::to_lowercase)
        .filter(|s| s.len() == 64)
        .ok_or("the release has no checksum to verify the download")?;
    Ok((url, sha, tag))
}

/// Saca `gprotab.exe` de los bytes de un zip (esté en la raíz o en una carpeta).
pub fn extract_exe(zip_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| format!("bad plugin archive: {e}"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map(|n| n.replace('\\', "/")).unwrap_or_default();
        if name == EXE_NAME || name.ends_with(&format!("/{EXE_NAME}")) {
            let mut out = Vec::new();
            f.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err(format!("{EXE_NAME} is not inside the plugin archive"))
}

// ---- Ejecución ----

fn run(args: &[&str]) -> Result<String, String> {
    let exe = exe_path().filter(|p| p.is_file()).ok_or("the tabs plugin is not installed")?;
    run_exe(&exe, args)
}

fn run_exe(exe: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(exe);
    cmd.arg("plugin").args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("could not run the plugin: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if stdout.trim().is_empty() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("the plugin gave no answer{}", if err.trim().is_empty() { String::new() } else { format!(": {}", err.trim()) }));
    }
    Ok(stdout)
}

/// Versión del complemento instalado (también comprueba que se puede ejecutar y que el protocolo coincide).
pub fn version() -> Result<String, String> {
    parse_info(&run(&["info"])?)
}

pub fn search(query: &str, page: u32) -> Result<Page, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Page::default());
    }
    parse_search(&run(&["search", q, &page.max(1).to_string()])?)
}

/// Descarga el tab `id` dentro de `dir` y devuelve la ruta del archivo.
pub fn download(id: &str, dir: &Path) -> Result<PathBuf, String> {
    let d = dir.to_string_lossy();
    parse_download(&run(&["download", id, &d])?)
}

/// Carpeta donde se guardan los tabs descargados.
pub fn downloads_dir() -> PathBuf {
    Settings::path().and_then(|p| p.parent().map(|d| d.join("downloads"))).unwrap_or_else(|| std::env::temp_dir().join("NeoGuitar-tabs"))
}

// ---- Instalación ----

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

/// Descarga el último release del complemento, verifica su SHA-256 y lo deja en la carpeta de plugins.
/// Devuelve la versión instalada.
pub fn install() -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = agent(Duration::from_secs(15))
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("could not look up the plugin: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("bad response: {e}"))?;
    let (asset_url, sha, _tag) = pick_asset(&body)?;

    let mut resp = agent(Duration::from_secs(300))
        .get(&asset_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let bytes = resp.body_mut().with_config().limit(60 * 1024 * 1024).read_to_vec().map_err(|e| format!("download failed: {e}"))?;
    let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if got != sha {
        return Err("the downloaded file does not match the published checksum; it was discarded".into());
    }
    let exe = extract_exe(&bytes)?;

    let dir = dir().ok_or("no settings folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let tmp = dir.join("gprotab.exe.new");
    std::fs::write(&tmp, &exe).map_err(|e| format!("could not write the plugin: {e}"))?;
    // Se comprueba que arranca y habla nuestro protocolo antes de dejarlo en su sitio.
    let checked = run_exe(&tmp, &["info"]).and_then(|t| parse_info(&t));
    match checked {
        Ok(version) => {
            let dest = dir.join(EXE_NAME);
            let _ = std::fs::remove_file(&dest);
            std::fs::rename(&tmp, &dest).map_err(|e| format!("could not install the plugin: {e}"))?;
            Ok(version)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
