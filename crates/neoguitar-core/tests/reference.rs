//! Compara nuestro lector con PyGuitarPro (implementación de referencia): notas por compás de cada pista.
//! El fixture se regenera con `python dump_ref.py` (ver tests/fixtures/pyguitarpro_notes.txt).

use std::path::Path;

#[test]
fn matches_pyguitarpro() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let text = std::fs::read_to_string(format!("{}/tests/fixtures/pyguitarpro_notes.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (mut files, mut bad) = (std::collections::BTreeSet::new(), Vec::new());
    let mut cache: Option<(String, neoguitar_core::model::Song)> = None;
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let mut it = line.split('|');
        let (file, track, counts) = (it.next().unwrap(), it.next().unwrap().parse::<usize>().unwrap(), it.next().unwrap());
        let expected: Vec<usize> = counts.split(',').filter(|s| !s.is_empty()).map(|s| s.parse().unwrap()).collect();
        if !Path::new(&format!("{root}/{file}")).exists() {
            continue; // muestra no incluida en el repositorio (derechos de autor)
        }
        if cache.as_ref().map(|c| c.0 != file).unwrap_or(true) {
            match neoguitar_core::load(Path::new(&format!("{root}/{file}"))) {
                Ok(s) => cache = Some((file.to_string(), s)),
                Err(e) => {
                    bad.push(format!("{file}: no se pudo leer ({e})"));
                    cache = None;
                    continue;
                }
            }
        }
        let Some((_, song)) = cache.as_ref().filter(|c| c.0 == file) else { continue };
        files.insert(file.to_string());
        let Some(t) = song.tracks.get(track) else {
            bad.push(format!("{file} pista {track}: falta"));
            continue;
        };
        let got: Vec<usize> = t.measures.iter().map(|m| m.beats.iter().flat_map(|b| &b.voices).map(|v| v.notes.len()).sum()).collect();
        if got != expected {
            let first = got.iter().zip(&expected).position(|(a, b)| a != b);
            bad.push(format!("{file} pista {track}: {} vs {} notas, primer compás distinto: {:?}", got.iter().sum::<usize>(), expected.iter().sum::<usize>(), first.map(|i| i + 1)));
        }
    }
    println!("{} archivos comparados", files.len());
    assert!(bad.is_empty(), "diferencias con PyGuitarPro:\n{}", bad.join("\n"));
}
