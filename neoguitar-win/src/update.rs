//! Actualizaciones: consulta el último release en GitHub, descarga el instalador verificando su SHA-256
//! y lo ejecuta en modo silencioso.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO: &str = "Fiambre/NeoGuitar";
const DOWNLOAD_PREFIX: &str = "https://github.com/Fiambre/NeoGuitar/releases/download/";
const USER_AGENT: &str = "NeoGuitar (https://github.com/Fiambre/NeoGuitar)";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Versión sin la "v" (p. ej. "0.3.0").
    pub version: String,
    /// Página del release.
    pub page: String,
    /// Instalador (`*-setup.exe`) con su tamaño y huella SHA-256, si el release los publica.
    pub installer: Option<Installer>,
    /// Resumen de cambios (texto del release).
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Installer {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// "0.3.0" → (0, 3, 0). Ignora un prefijo "v" y sufijos como "-beta".
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?))
}

/// ¿`latest` es una versión posterior a `current`?
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// ¿El tag es de un release de Windows? Son `vX.Y.Z` (los únicos que entiende NeoGuitar 0.3.0, que pide
/// `releases/latest`) y también se acepta `win-vX.Y.Z`. Los de otras plataformas (`android-v…`) no cuentan.
fn is_windows_tag(tag: &str) -> bool {
    let t = tag.strip_prefix("win-").unwrap_or(tag);
    t.starts_with('v') && parse_version(t).is_some()
}

/// Elige, de la lista de releases del repositorio, el más nuevo de Windows (ignora borradores y prerelease).
pub fn parse_windows_releases(json: &str) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    let list = v.as_array().ok_or("bad response: expected a list of releases")?;
    let mut best: Option<((u32, u32, u32), &serde_json::Value)> = None;
    for item in list {
        if item["draft"].as_bool() == Some(true) || item["prerelease"].as_bool() == Some(true) {
            continue;
        }
        let Some(tag) = item["tag_name"].as_str().filter(|t| is_windows_tag(t)) else { continue };
        let Some(ver) = parse_version(tag.strip_prefix("win-").unwrap_or(tag)) else { continue };
        if best.is_none_or(|(b, _)| ver > b) {
            best = Some((ver, item));
        }
    }
    best.map(|(_, item)| parse_release(&item.to_string())).transpose()
}

/// Interpreta un release (objeto) de la API de GitHub.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("the release has no tag")?;
    let installer = v["assets"].as_array().and_then(|assets| {
        assets.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            if !name.ends_with("-setup.exe") {
                return None;
            }
            let url = a["browser_download_url"].as_str()?.to_string();
            let sha256 = a["digest"].as_str()?.strip_prefix("sha256:")?.to_lowercase();
            Some(Installer { url, size: a["size"].as_u64().unwrap_or(0), sha256 })
        })
    });
    Ok(Release {
        version: tag.strip_prefix("win-").unwrap_or(tag).trim_start_matches('v').to_string(),
        page: v["html_url"].as_str().unwrap_or("https://github.com/Fiambre/NeoGuitar/releases").to_string(),
        installer,
        notes: v["body"].as_str().unwrap_or("").to_string(),
    })
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

/// Consulta el último release de Windows; `Some` solo si es más nuevo que la versión instalada.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=30");
    let mut resp = agent(Duration::from_secs(12))
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("could not check for updates: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("bad response: {e}"))?;
    let Some(release) = parse_windows_releases(&body)? else { return Ok(None) };
    // NEOGUITAR_CURRENT_VERSION permite probar el aviso sin publicar una versión nueva.
    let current = std::env::var("NEOGUITAR_CURRENT_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    Ok(is_newer(&current, &release.version).then_some(release))
}

/// Descarga el instalador a `dest` y comprueba su SHA-256. Si no coincide, borra el archivo y falla.
pub fn download(installer: &Installer, dest: &Path) -> Result<(), String> {
    if !installer.url.starts_with(DOWNLOAD_PREFIX) {
        return Err("the installer is not hosted in the project's releases".into());
    }
    if installer.sha256.len() != 64 {
        return Err("the release has no checksum to verify the download".into());
    }
    let result = (|| {
        let mut resp = agent(Duration::from_secs(1200))
            .get(&installer.url)
            .header("User-Agent", USER_AGENT)
            .call()
            .map_err(|e| format!("download failed: {e}"))?;
        let mut reader = resp.body_mut().with_config().limit(400 * 1024 * 1024).reader();
        let mut file = std::fs::File::create(dest).map_err(|e| format!("could not write {}: {e}", dest.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("download failed: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(|e| format!("could not write the file: {e}"))?;
        }
        file.flush().map_err(|e| e.to_string())?;
        let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if got != installer.sha256 {
            return Err("the downloaded file does not match the published checksum; it was discarded".into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

/// Ruta temporal del instalador descargado.
pub fn installer_path(version: &str) -> PathBuf {
    std::env::temp_dir().join(format!("NeoGuitar-{version}-setup.exe"))
}

/// ¿Este ejecutable salió del instalador? (junto a él está el desinstalador de Inno Setup).
/// La versión portable no se actualiza sola.
pub fn is_installed_copy() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("unins000.exe").exists())).unwrap_or(false)
}

/// Ejecuta el instalador en modo silencioso; cierra y reabre la aplicación por su cuenta.
pub fn run_installer(path: &Path) -> Result<(), String> {
    std::process::Command::new(path)
        .args(["/SILENT", "/NORESTART", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS"])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the installer: {e}"))
}

/// SHA-256 de unos bytes, en hexadecimal (para las pruebas).
pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}
