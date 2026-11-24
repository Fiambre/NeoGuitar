use neoguitar::model::*;

fn has(name: &str) -> bool {
    std::path::Path::new(&format!("{}/samples/{name}", env!("CARGO_MANIFEST_DIR"))).exists()
}

fn load(name: &str) -> Song {
    let path = format!("{}/samples/{name}", env!("CARGO_MANIFEST_DIR"));
    neoguitar::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Carga una muestra; si no está (no se publican las tablaturas de terceros), el test se omite.
macro_rules! load_or_skip {
    ($name:expr) => {{
        let path = format!("{}/samples/{}", env!("CARGO_MANIFEST_DIR"), $name);
        if !std::path::Path::new(&path).exists() {
            eprintln!("sin la muestra {}, se omite", $name);
            return;
        }
        load($name)
    }};
}

fn dump(song: &Song) {
    println!("{} | {} | {} | tempo {} | {} compases", song.format, song.title, song.artist, song.tempo, song.headers.len());
    for t in &song.tracks {
        let notes: usize = t.measures.iter().flat_map(|m| &m.beats).flat_map(|b| &b.voices).map(|v| v.notes.len()).sum();
        println!("  pista {} {:?} cuerdas {:?} perc {} notas {}", t.number, t.name, t.strings, t.is_percussion, notes);
    }
}

fn check(song: &Song) {
    assert!(!song.tracks.is_empty());
    for t in &song.tracks {
        assert_eq!(t.measures.len(), song.headers.len());
        for (m, h) in t.measures.iter().zip(&song.headers) {
            for b in &m.beats {
                assert!(b.start >= h.start && b.start < h.start + h.length(), "pulso fuera del compás {}", h.number);
                for v in &b.voices {
                    for n in &v.notes {
                        assert!(n.string as usize >= 1 && n.string as usize <= t.strings.len());
                    }
                }
            }
        }
    }
}

#[test]
fn gp3() {
    let s = load_or_skip!("nirvana-aneurysm.gp3");
    dump(&s);
    check(&s);
}

#[test]
fn gp5() {
    let s = load_or_skip!("nirvana-breed_9.gp5");
    dump(&s);
    check(&s);
}

#[test]
fn gpx6() {
    let s = load_or_skip!("sabotage-22077.gpx");
    dump(&s);
    check(&s);
}

#[test]
fn gp7() {
    let s = load_or_skip!("Metallica - Nothing Else Matters (ver 6).gp");
    dump(&s);
    check(&s);
}

#[test]
fn bend_stats() {
    for f in ["nirvana-breed_9.gp5", "nirvana-aneurysm.gp3", "sabotage-22077.gpx", "Metallica - Nothing Else Matters (ver 6).gp"] {
        if !has(f) {
            continue;
        }
        let s = load(f);
        let mut hist = std::collections::BTreeMap::new();
        for t in &s.tracks {
            for n in t.measures.iter().flat_map(|m| &m.beats).flat_map(|b| &b.voices).flat_map(|v| &v.notes) {
                if let Some(b) = &n.effect.bend {
                    *hist.entry((b.max_semitones() * 100.0).round() as i32).or_insert(0) += 1;
                }
            }
        }
        println!("{f}: max del bend (centésimas de semitono; 200 = tono entero) -> cantidad: {hist:?}");
    }
}

#[test]
fn bend_locations() {
    for (f, track) in [("Metallica - Nothing Else Matters (ver 6).gp", 3usize), ("nirvana-breed_9.gp5", 1)] {
        if !has(f) {
            continue;
        }
        let s = load(f);
        let t = &s.tracks[track - 1];
        let mut found = Vec::new();
        for (i, m) in t.measures.iter().enumerate() {
            let n = m.beats.iter().flat_map(|b| &b.voices).flat_map(|v| &v.notes).filter(|n| n.effect.bend.is_some()).count();
            if n > 0 {
                found.push((i + 1, n));
            }
        }
        println!("{f} pista {track}: (compás, bends) {:?}", &found[..found.len().min(12)]);
    }
}

/// Parsea todos los archivos de `samples/extra` (casos mínimos de GP3/GP4/GP5).
#[test]
fn extra_samples() {
    let dir = format!("{}/samples/extra", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| ["gp3", "gp4", "gp5"].contains(&x.to_string_lossy().to_lowercase().as_str())))
        .collect();
    files.sort();
    let mut failures = Vec::new();
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        match neoguitar::load(f) {
            Ok(song) => {
                let notes: usize = song
                    .tracks
                    .iter()
                    .flat_map(|t| &t.measures)
                    .flat_map(|m| &m.beats)
                    .flat_map(|b| &b.voices)
                    .map(|v| v.notes.len())
                    .sum();
                let mut problems = Vec::new();
                for t in &song.tracks {
                    if t.measures.len() != song.headers.len() {
                        problems.push("compases != cabeceras".to_string());
                    }
                    for (m, h) in t.measures.iter().zip(&song.headers) {
                        for b in &m.beats {
                            if b.start < h.start || b.start >= h.start + h.length() {
                                problems.push(format!("pulso fuera del compás {}", h.number));
                            }
                        }
                    }
                }
                println!("{:<34} {:<18} {} pistas, {} compases, {} notas {}", name, song.format, song.tracks.len(), song.headers.len(), notes, if problems.is_empty() { "" } else { "  PROBLEMAS" });
                if !problems.is_empty() {
                    failures.push(format!("{name}: {problems:?}"));
                }
            }
            Err(e) => {
                println!("{:<34} ERROR: {e}", name);
                failures.push(format!("{name}: {e}"));
            }
        }
    }
    assert!(failures.is_empty(), "fallaron {} de {}:\n{}", failures.len(), files.len(), failures.join("\n"));
}

#[test]
fn lyrics_and_chords() {
    let dir = format!("{}/samples/extra", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.push(format!("{}/samples/nirvana-aneurysm.gp3", env!("CARGO_MANIFEST_DIR")).into());
    files.push(format!("{}/samples/nirvana-breed_9.gp5", env!("CARGO_MANIFEST_DIR")).into());
    files.sort();
    for f in files {
        let Ok(song) = neoguitar::load(&f) else { continue };
        let chords = song.tracks.iter().flat_map(|t| &t.measures).flat_map(|m| &m.beats).filter(|b| b.chord.is_some()).count();
        let lyr = song.lyrics.as_ref().map(|l| l.syllables().len()).unwrap_or(0);
        if chords > 0 || lyr > 0 {
            println!("{:<34} acordes {chords:<3} sílabas de letra {lyr}", f.file_name().unwrap().to_string_lossy());
        }
    }
}

#[test]
fn demo_tracks_debug() {
    let song = neoguitar::load(format!("{}/samples/extra/Demo v5.gp5", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for t in &song.tracks {
        println!("pista {} {:?} perc={} cuerdas={:?} prog={} vol={} pan={}", t.number, t.name, t.is_percussion, t.strings, t.program, t.volume, t.pan);
    }
    if let Some(l) = &song.lyrics {
        println!("letra: pista {} desde compás {}: {:?}", l.track + 1, l.from_measure + 1, l.text);
    }
}

#[test]
fn ascii_export() {
    let song = neoguitar::load(format!("{}/samples/extra/Demo v5.gp5", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let text = neoguitar::ascii::ascii_tab(&song, &song.tracks[0], 100);
    println!("{}", text.lines().take(14).collect::<Vec<_>>().join("
"));
    assert!(text.contains("Tuning: E4 B3 G3 D3 A2 E2"));
    // 6 cuerdas por bloque, todas de igual largo.
    let rows: Vec<&str> = text.lines().filter(|l| l.contains('|') && l.contains('-')).collect();
    assert!(rows.len() >= 6);
    for chunk in rows.chunks(6).filter(|c| c.len() == 6) {
        assert!(chunk.iter().all(|r| r.chars().count() == chunk[0].chars().count()), "{chunk:?}");
    }
}

#[test]
fn settings_roundtrip() {
    use neoguitar::settings::Settings;
    let mut s = Settings::default();
    s.zoom = 1.4;
    s.show_staff = false;
    s.speed = 80.0;
    s.metronome = true;
    s.count_in = true;
    s.trainer_on = true;
    s.trainer_step = 7;
    s.soundfont = Some("C:/a b/fuente.sf2".into());
    s.add_recent(std::path::Path::new("C:/x/uno.gp5"));
    s.add_recent(std::path::Path::new("C:/x/dos.gp5"));
    s.add_recent(std::path::Path::new("C:/x/uno.gp5")); // sube al frente sin duplicarse
    let back = Settings::parse(&s.serialize());
    assert_eq!(back, s);
    assert_eq!(back.recent.len(), 2);
    assert_eq!(back.recent[0], std::path::PathBuf::from("C:/x/uno.gp5"));
    // Valores fuera de rango o basura se ajustan sin fallar.
    let bad = Settings::parse("zoom=99\nspeed=abc\nbasura\nrecent=\ntrainer_step=0\n");
    assert_eq!(bad.zoom, 2.0);
    assert_eq!(bad.speed, 100.0);
    assert_eq!(bad.trainer_step, 1);
    assert!(bad.recent.is_empty());
}

#[test]
fn sounding_notes_at_beat() {
    let s = load("extra/Demo v5.gp5");
    let mut checked = 0;
    for t in s.tracks.iter().filter(|t| !t.is_percussion) {
        for (mi, m) in t.measures.iter().enumerate() {
            for b in &m.beats {
                for v in b.voices.iter().filter(|v| !v.empty && !v.notes.is_empty()) {
                    let now = t.sounding_notes(mi, b.start);
                    for n in v.notes.iter().filter(|n| !n.effect.dead) {
                        assert!(now.contains(&(n.string, n.fret)), "falta la nota {:?} en el compás {}", (n.string, n.fret), mi + 1);
                        checked += 1;
                    }
                    // Pasada la duración del pulso, la nota ya no suena (salvo que otro pulso la sostenga).
                    let after = t.sounding_notes(mi, b.start + v.duration.time() + 1);
                    assert!(after.len() <= now.len() + 12);
                }
            }
        }
    }
    assert!(checked > 50, "se revisaron pocas notas: {checked}");
    // Fuera de rango no hay nada.
    assert!(s.tracks[0].sounding_notes(10_000, 0).is_empty());
}

#[test]
fn key_signature_transposition() {
    // Do mayor (0): +2 = Re (2#), -1 = Si (5#), +3 = Mib (3b); Sol mayor (1#) +1 = Lab (4b).
    assert_eq!(transpose_key(0, 2), 2);
    assert_eq!(transpose_key(0, -1), 5);
    assert_eq!(transpose_key(0, 3), -3);
    assert_eq!(transpose_key(1, 1), -4);
    // Una octava no cambia nada, y volver atrás restituye la armadura.
    for k in -5..=5i8 {
        assert_eq!(transpose_key(k, 12), k);
        for n in -12..=12 {
            assert_eq!(transpose_key(transpose_key(k, n), -n), transpose_key(k, 0));
        }
    }
}
