//! Asociación de archivos Guitar Pro con NeoGuitar en Windows (solo para el usuario actual, sin administrador).
//! Escribe en `HKCU\Software\Classes` usando la herramienta `reg` del sistema.

use std::os::windows::process::CommandExt;
use std::process::Command;

const PROG_ID: &str = "NeoGuitar.GuitarPro";
const BASE: &str = r"HKCU\Software\Classes";
pub const EXTENSIONS: [&str; 5] = ["gp3", "gp4", "gp5", "gpx", "gp"];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn reg(args: &[&str]) -> Result<String, String> {
    let out = Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Registra NeoGuitar como programa para los formatos Guitar Pro.
pub fn associate() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.display().to_string();
    let prog = format!(r"{BASE}\{PROG_ID}");
    reg(&["add", &prog, "/ve", "/d", "Guitar Pro file", "/f"])?;
    reg(&["add", &format!(r"{prog}\DefaultIcon"), "/ve", "/d", &format!("\"{exe}\",0"), "/f"])?;
    reg(&["add", &format!(r"{prog}\shell\open\command"), "/ve", "/d", &format!("\"{exe}\" \"%1\""), "/f"])?;
    for ext in EXTENSIONS {
        let key = format!(r"{BASE}\.{ext}");
        reg(&["add", &key, "/ve", "/d", PROG_ID, "/f"])?;
        reg(&["add", &format!(r"{key}\OpenWithProgids"), "/v", PROG_ID, "/t", "REG_NONE", "/f"])?;
    }
    Ok(())
}
