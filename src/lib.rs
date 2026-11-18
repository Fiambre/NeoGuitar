pub mod gp;
pub mod gpx;
pub mod ascii;
pub mod audio;
pub mod model;
pub mod gm;
pub mod playback;
pub mod settings;

use std::path::Path;

/// Lee un archivo de partitura y detecta el formato por su contenido.
pub fn load(path: impl AsRef<Path>) -> Result<model::Song, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(parse(&bytes)?)
}

/// Detecta el formato por los primeros bytes: ZIP (GP7+), BCFZ/BCFS (GP6) o cabecera GP3-GP5.
pub fn parse(bytes: &[u8]) -> Result<model::Song, gp::GpError> {
    match bytes.get(..4) {
        Some(b"PK") => gpx::parse_gp7(bytes),
        Some(b"BCFZ") | Some(b"BCFS") => gpx::parse_gpx6(bytes),
        _ => gp::parse(bytes),
    }
}
