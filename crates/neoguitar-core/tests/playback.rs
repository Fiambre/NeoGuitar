use neoguitar_core::model::*;
use neoguitar_core::playback::*;
use rustysynth::SoundFont;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;

fn load(name: &str) -> Song {
    neoguitar_core::load(format!("{}/samples/{name}", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap()
}

/// Carga una muestra; si no está (no se publican las tablaturas de terceros), el test se omite.
macro_rules! load_or_skip {
    ($name:expr) => {{
        let path = format!("{}/samples/{}", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."), $name);
        if !std::path::Path::new(&path).exists() {
            eprintln!("sin la muestra {}, se omite", $name);
            return;
        }
        load($name)
    }};
}

fn header(start: u64, open: bool, close: i32, alt: u8) -> MeasureHeader {
    MeasureHeader {
        number: 0,
        start,
        numerator: 4,
        denominator: 4,
        repeat_open: open,
        repeat_close: close,
        repeat_alternative: alt,
        marker: None,
        key_signature: 0,
        tempo: 120,
        tempo_changes: Vec::new(),
        triplet_feel: TripletFeel::None,
    }
}

#[test]
fn repeat_order() {
    let h = |o, c, a| header(0, o, c, a);
    // |: 0 1 :| 2  -> 0 1 0 1 2
    let hs = vec![h(true, 0, 0), h(false, 1, 0), h(false, 0, 0)];
    assert_eq!(play_order(&hs), vec![0, 1, 0, 1, 2]);
    // |: 0 | 1(final 1) :| 2(final 2) | 3 -> 0 1 0 2 3
    let hs = vec![h(true, 0, 0), h(false, 1, 0b01), h(false, 0, 0b10), h(false, 0, 0)];
    assert_eq!(play_order(&hs), vec![0, 1, 0, 2, 3]);
    // sin repeticiones
    assert_eq!(play_order(&[h(false, 0, 0), h(false, 0, 0)]), vec![0, 1]);
}

#[test]
fn timeline_samples() {
    for f in ["nirvana-aneurysm.gp3", "nirvana-breed_9.gp5", "sabotage-22077.gpx", "Metallica - Nothing Else Matters (ver 6).gp"] {
        if !std::path::Path::new(&format!("{}/samples/{f}", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).exists() {
            eprintln!("sin la muestra {f}, se omite");
            continue;
        }
        let s = load(f);
        let tl = Timeline::build(&s);
        println!("{f}:{} eventos, {:.1} s, {} compases sonando", tl.events.len(), tl.total, tl.segs.len());
        assert!(tl.total > 10.0, "{f}");
        let ons = tl.events.iter().filter(|e| matches!(e.kind, Kind::NoteOn { .. })).count();
        let offs = tl.events.iter().filter(|e| matches!(e.kind, Kind::NoteOff { .. })).count();
        assert_eq!(ons, offs, "{f}: notas encendidas y apagadas deben coincidir");
        assert!(tl.events.windows(2).all(|w| w[0].time <= w[1].time));
        let (m, _) = tl.position(tl.total / 2.0).unwrap();
        assert!(m < s.headers.len());
    }
}

#[test]
fn render_offline() {
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(mut file) = std::fs::File::open(&sf) else {
        eprintln!("sin SoundFont, se omite");
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let song = load_or_skip!("nirvana-breed_9.gp5");
    let tl = Arc::new(Timeline::build(&song));

    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font, 44100, shared.clone(), rx).unwrap();
    tx.send(Cmd::SetTimeline(tl)).unwrap();
    tx.send(Cmd::Play).unwrap();

    let seconds = 20;
    let frames = 44100 * seconds;
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let mut pcm: Vec<i16> = Vec::with_capacity(frames * 2);
    let mut peak = 0f32;
    for _ in 0..(frames / 512) {
        engine.render(&mut l, &mut r);
        for i in 0..512 {
            peak = peak.max(l[i].abs()).max(r[i].abs());
            pcm.push((l[i].clamp(-1.0, 1.0) * 32767.0) as i16);
            pcm.push((r[i].clamp(-1.0, 1.0) * 32767.0) as i16);
        }
    }
    println!("pico = {peak:.3}, posición final = {:.2} s", shared.position());
    assert!(peak > 0.05, "el render quedó en silencio");
    assert!(shared.playing.load(Ordering::Relaxed));
    assert!(shared.position() > 15.0);

    // Se guarda un WAV para escucharlo.
    let path = format!("{}/target/breed-render.wav", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut wav = Vec::new();
    let data_len = (pcm.len() * 2) as u32;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&44100u32.to_le_bytes());
    wav.extend_from_slice(&(44100u32 * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in &pcm {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(&path, wav).unwrap();
    println!("WAV: {path}");
}

fn count(tl: &Timeline, f: impl Fn(&Kind) -> bool) -> usize {
    tl.events.iter().filter(|e| f(&e.kind)).count()
}

#[test]
fn expressive_events() {
    // Nothing Else Matters tiene bends, slides, armónicos, trinos y notas de adorno.
    let song = load_or_skip!("Metallica - Nothing Else Matters (ver 6).gp");
    let tl = Timeline::build(&song);
    let bends = count(&tl, |k| matches!(k, Kind::Bend(_)));
    let programs = count(&tl, |k| matches!(k, Kind::Program(_)));
    println!("eventos de bend: {bends}, cambios de programa auxiliares: {programs}");
    assert!(bends > 100, "debería haber pitch bends");
    assert!(programs > 0, "los bends deben ir por canales auxiliares");
    // Los canales auxiliares son melódicos y distintos del 9.
    assert!(tl.events.iter().all(|e| !matches!(e.kind, Kind::Bend(_)) || e.channel != 9));

    // El metrónomo existe y está marcado.
    assert!(tl.events.iter().any(|e| e.track == METRONOME_TRACK));

    // El MIDI exportado es un SMF válido.
    let midi = tl.to_midi(u64::MAX, false);
    assert_eq!(&midi[..4], b"MThd");
    assert_eq!(&midi[14..18], b"MTrk");
    let len = u32::from_be_bytes([midi[18], midi[19], midi[20], midi[21]]) as usize;
    assert_eq!(midi.len(), 22 + len);
    std::fs::write(format!("{}/target/nothing-else-matters.mid", concat!(env!("CARGO_MANIFEST_DIR"), "/../..")), &midi).unwrap();
}

#[test]
fn tempo_changes_inside_measure() {
    let mut song = load_or_skip!("nirvana-breed_9.gp5");
    let base = Timeline::build(&song);
    // Se fuerza un cambio de tempo a mitad del compás 3: el total debe cambiar.
    let h = &mut song.headers[2];
    let mid = h.start + h.length() / 2;
    h.tempo_changes.push((mid, h.tempo * 2));
    let changed = Timeline::build(&song);
    println!("total {:.2} -> {:.2}", base.total, changed.total);
    assert!(changed.total < base.total - 0.1);
    // Y el compás se divide en dos tramos de tiempo.
    assert!(changed.segs.iter().filter(|s| s.measure == 2).count() >= 2);
}

#[test]
fn swing_changes_timing() {
    let mut song = load_or_skip!("nirvana-breed_9.gp5");
    let straight = Timeline::build(&song);
    for h in &mut song.headers {
        h.triplet_feel = TripletFeel::Eighth;
    }
    let swung = Timeline::build(&song);
    let differs = straight.events.len() != swung.events.len()
        || straight.events.iter().zip(&swung.events).any(|(a, b)| (a.time - b.time).abs() > 0.01);
    assert!(differs, "el swing debe mover algunas notas");
}

/// Renderiza `seconds` desde el compás `measure` (base 1) y guarda un WAV; devuelve el pico.
fn render_wav(song: &Song, measure: usize, seconds: usize, name: &str) -> Option<f32> {
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut file = std::fs::File::open(&sf).ok()?;
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let tl = Arc::new(Timeline::build(song));
    let from = tl.time_of_measure(measure - 1).unwrap();

    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font, 44100, shared, rx).unwrap();
    tx.send(Cmd::SetTimeline(tl)).unwrap();
    tx.send(Cmd::Seek(from)).unwrap();
    tx.send(Cmd::Play).unwrap();

    let frames = 44100 * seconds;
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let mut pcm: Vec<i16> = Vec::with_capacity(frames * 2);
    let mut peak = 0f32;
    for _ in 0..(frames / 512) {
        engine.render(&mut l, &mut r);
        for i in 0..512 {
            peak = peak.max(l[i].abs()).max(r[i].abs());
            pcm.push((l[i].clamp(-1.0, 1.0) * 32767.0) as i16);
            pcm.push((r[i].clamp(-1.0, 1.0) * 32767.0) as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 44100u32.to_le_bytes().to_vec(), (44100u32 * 4).to_le_bytes().to_vec(), 4u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
        wav.extend(v);
    }
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in &pcm {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(format!("{}/target/{name}", concat!(env!("CARGO_MANIFEST_DIR"), "/../..")), wav).unwrap();
    Some(peak)
}

#[test]
fn render_guitar_solo() {
    let song = load_or_skip!("Metallica - Nothing Else Matters (ver 6).gp");
    let Some(peak) = render_wav(&song, 72, 40, "nothing-else-matters-solo.wav") else {
        eprintln!("sin SoundFont, se omite");
        return;
    };
    println!("solo: pico = {peak:.3}");
    assert!(peak > 0.05);
}

#[test]
fn count_in_and_trainer() {
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(mut file) = std::fs::File::open(&sf) else {
        eprintln!("sin SoundFont, se omite");
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let song = load_or_skip!("nirvana-breed_9.gp5");
    let tl = Arc::new(Timeline::build(&song));

    // Cuenta regresiva: 4 tiempos de 0,5 s; la canción no avanza hasta que termina.
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font.clone(), 44100, shared.clone(), rx).unwrap();
    tx.send(Cmd::SetTimeline(tl.clone())).unwrap();
    tx.send(Cmd::PlayCountIn { beats: 4, beat_secs: 0.5, delay: 0.0 }).unwrap();
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let render = |engine: &mut Engine, secs: f64, l: &mut Vec<f32>, r: &mut Vec<f32>| -> f32 {
        let mut peak = 0f32;
        for _ in 0..((secs * 44100.0) as usize / 512) {
            engine.render(l, r);
            peak = l.iter().chain(r.iter()).fold(peak, |p, v| p.max(v.abs()));
        }
        peak
    };
    let peak = render(&mut engine, 1.5, &mut l, &mut r);
    println!("cuenta: pico {peak:.3}, posición {:.3}", shared.position());
    assert!(peak > 0.02, "la cuenta debe sonar");
    assert!(shared.position() < 0.001, "la canción no debe avanzar durante la cuenta");
    render(&mut engine, 1.0, &mut l, &mut r);
    assert!(shared.position() > 0.2, "después de la cuenta la canción avanza: {}", shared.position());

    // Espera previa: 3 s de silencio antes de los clics, y la canción empieza recién después de ellos.
    let (tx, rx) = channel();
    let shared2 = Arc::new(Shared::default());
    shared2.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine2 = Engine::new(font.clone(), 44100, shared2.clone(), rx).unwrap();
    tx.send(Cmd::SetTimeline(tl.clone())).unwrap();
    tx.send(Cmd::PlayCountIn { beats: 2, beat_secs: 0.5, delay: 3.0 }).unwrap();
    let quiet = render(&mut engine2, 2.8, &mut l, &mut r);
    assert!(quiet < 0.001, "durante la espera no debe sonar nada: {quiet}");
    let clicks = render(&mut engine2, 1.0, &mut l, &mut r);
    assert!(clicks > 0.02, "después de la espera suenan los clics");
    assert!(shared2.position() < 0.001, "la canción sigue sin avanzar");
    render(&mut engine2, 1.0, &mut l, &mut r);
    assert!(shared2.position() > 0.1, "y luego arranca la canción: {}", shared2.position());

    // Entrenador: un bucle corto sube la velocidad en cada vuelta hasta el máximo.
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font, 44100, shared.clone(), rx).unwrap();
    tx.send(Cmd::SetTimeline(tl)).unwrap();
    tx.send(Cmd::SetSpeed(0.6)).unwrap();
    shared.set_loop(0.0, 2.0);
    shared.loop_on.store(true, Ordering::Relaxed);
    shared.trainer_on.store(true, Ordering::Relaxed);
    shared.trainer_step.store(10, Ordering::Relaxed);
    shared.trainer_max.store(90, Ordering::Relaxed);
    tx.send(Cmd::Play).unwrap();
    render(&mut engine, 12.0, &mut l, &mut r);
    let pct = shared.speed_pct.load(Ordering::Relaxed);
    println!("entrenador: velocidad final {pct}%");
    assert_eq!(pct, 90, "debe llegar al máximo y quedarse ahí");
}

#[test]
fn export_wav_offline() {
    use neoguitar_core::audio::render_wav;
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(mut file) = std::fs::File::open(&sf) else {
        eprintln!("sin SoundFont, se omite");
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let song = load("extra/Demo v5.gp5");
    let tl = Arc::new(Timeline::build(&song));
    let out = format!("{}/target/export-test.wav", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    render_wav(font, tl.clone(), u64::MAX, &[100; 64], false, 1.0, std::path::Path::new(&out)).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 44 + data_len);
    let secs = data_len as f64 / (44100.0 * 4.0);
    println!("WAV: {secs:.1} s (la canción dura {:.1} s)", tl.total);
    assert!(secs > tl.total && secs < tl.total + 3.0, "duración inesperada: {secs}");
    let peak = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap();
    assert!(peak > 1000, "el WAV quedó en silencio");
    assert!(peak <= 32767);
}

#[test]
fn transposition_shifts_pitched_notes_only() {
    let song = load("extra/Demo v5.gp5");
    let tl = Timeline::build(&song);
    let up = tl.transposed(3);
    let down = tl.transposed(-5);
    assert_eq!(tl.events.len(), up.events.len());
    assert_eq!(tl.total, up.total);
    let (mut pitched, mut drums) = (0, 0);
    for ((a, b), c) in tl.events.iter().zip(&up.events).zip(&down.events) {
        match (a.kind, b.kind, c.kind) {
            (Kind::NoteOn { key: k0, velocity: v0 }, Kind::NoteOn { key: k1, velocity: v1 }, Kind::NoteOn { key: k2, .. }) => {
                assert_eq!(v0, v1);
                if a.channel == 9 {
                    assert_eq!((k0, k1, k2), (k0, k0, k0), "la percusión no se transpone");
                    drums += 1;
                } else {
                    assert_eq!(k1 as i32, (k0 as i32 + 3).min(127));
                    assert_eq!(k2 as i32, (k0 as i32 - 5).max(0));
                    pitched += 1;
                }
            }
            (Kind::NoteOff { key: k0 }, Kind::NoteOff { key: k1 }, _) if a.channel != 9 => assert_eq!(k1 as i32, (k0 as i32 + 3).min(127)),
            (x, y, _) => assert_eq!(format!("{x:?}"), format!("{y:?}")),
        }
    }
    assert!(pitched > 100 && drums > 10, "melódicas {pitched}, percusión {drums}");
}

#[test]
fn replace_timeline_keeps_position_and_playing() {
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(mut file) = std::fs::File::open(&sf) else {
        eprintln!("sin SoundFont, se omite");
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let song = load("extra/Demo v5.gp5");
    let tl = Arc::new(Timeline::build(&song));
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font, 44100, shared.clone(), rx).unwrap();
    tx.send(Cmd::SetTimeline(tl.clone())).unwrap();
    tx.send(Cmd::Seek(5.0)).unwrap();
    tx.send(Cmd::Play).unwrap();
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    for _ in 0..100 {
        engine.render(&mut l, &mut r);
    }
    let before = shared.position();
    tx.send(Cmd::ReplaceTimeline(Arc::new(tl.transposed(2)))).unwrap();
    for _ in 0..10 {
        engine.render(&mut l, &mut r);
    }
    let after = shared.position();
    assert!(before > 5.5, "la reproducción avanzó: {before}");
    assert!(after > before && after < before + 0.5, "la posición se conserva: {before} -> {after}");
    assert!(shared.playing.load(Ordering::Relaxed), "sigue reproduciendo");
}

#[test]
fn live_mix_changes_pan_and_instrument() {
    use neoguitar_core::playback::{Event, MIX_FILE};
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(mut file) = std::fs::File::open(&sf) else {
        eprintln!("sin SoundFont, se omite");
        return;
    };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let song = load("extra/Demo v5.gp5");
    let tl = Arc::new(Timeline::build(&song));
    let track = song.tracks.iter().position(|t| !t.is_percussion).unwrap();
    let first: &Event = tl.events.iter().find(|e| e.track as usize == track && matches!(e.kind, Kind::NoteOn { .. })).unwrap();
    let start = (first.time - 0.05).max(0.0);

    // Renderiza 2 s desde la primera nota de la pista con la mezcla indicada y devuelve (pico izq., pico der., muestras).
    let run = |pan: u8, program: u8| -> (f32, f32, Vec<f32>) {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared::default());
        shared.mask.store(1u64 << track, Ordering::Relaxed);
        shared.mix_pan[track].store(pan, Ordering::Relaxed);
        shared.mix_program[track].store(program, Ordering::Relaxed);
        let mut engine = Engine::new(font.clone(), 44100, shared, rx).unwrap();
        tx.send(Cmd::SetTimeline(tl.clone())).unwrap();
        tx.send(Cmd::Seek(start)).unwrap();
        tx.send(Cmd::Play).unwrap();
        let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
        let (mut pl, mut pr, mut all) = (0f32, 0f32, Vec::new());
        for _ in 0..(44100 * 2 / 512) {
            engine.render(&mut l, &mut r);
            pl = l.iter().fold(pl, |m, v| m.max(v.abs()));
            pr = r.iter().fold(pr, |m, v| m.max(v.abs()));
            all.extend_from_slice(&l);
        }
        (pl, pr, all)
    };
    let (l0, r0, _) = run(0, MIX_FILE);
    let (l1, r1, _) = run(127, MIX_FILE);
    println!("pan izq: {l0:.3}/{r0:.3}  pan der: {l1:.3}/{r1:.3}");
    assert!(l0 > 0.01 && r1 > 0.01, "debe sonar");
    assert!(l0 > r0 * 4.0, "todo a la izquierda");
    assert!(r1 > l1 * 4.0, "todo a la derecha");

    let (_, _, a) = run(64, MIX_FILE);
    let (_, _, b) = run(64, 40); // violín
    let diff: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum();
    println!("diferencia de audio entre instrumentos: {diff:.1}");
    assert!(diff > 5.0, "cambiar el instrumento debe cambiar el sonido");
}

#[test]
fn technique_channels_and_strum() {
    let song = load("extra/Harmonics.gp5");
    let tl = Timeline::build(&song);
    assert!(!tl.tech.is_empty(), "harmonics should get an auxiliary channel");
    for &(track, ch, prog) in &tl.tech {
        assert_eq!(prog, 31);
        assert!(!tl.channels.contains(&ch) && ch != 9, "auxiliary channel must be free");
        assert!(tl.setup.iter().any(|&(c, cmd, d1, _)| c == ch && cmd == 0xC0 && d1 == 31));
        assert!(tl.events.iter().any(|e| e.channel == ch && e.track as usize == track && matches!(e.kind, Kind::NoteOn { .. })));
    }
    // The transposed copy keeps the auxiliary channels (the mixer needs them).
    assert_eq!(tl.transposed(2).tech, tl.tech);

    // Chords are strummed: notes of the same chord do not all start at the same instant.
    let song = load("extra/Chords.gp5");
    let tl = Timeline::build(&song);
    let mut starts: Vec<f64> = tl.events.iter().filter(|e| e.channel != 9 && matches!(e.kind, Kind::NoteOn { .. })).map(|e| e.time).collect();
    starts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert!(starts.windows(2).any(|w| w[1] - w[0] > 0.0 && w[1] - w[0] < 0.05), "expected staggered chord notes");
}

#[test]
fn amp_synth_and_tremolo_bar() {
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let Ok(mut file) = std::fs::File::open(&sf) else { return };
    let font = Arc::new(SoundFont::new(&mut file).unwrap());

    // A distortion guitar goes through the amp synth: audible, finite and not clipping.
    let mut song = load("extra/Chords.gp5");
    for t in song.tracks.iter_mut().filter(|t| !t.is_percussion) {
        t.program = 30;
    }
    let tl = Arc::new(Timeline::build(&song));
    assert_ne!(tl.amp, 0);
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font, 44100, shared, rx).unwrap();
    tx.send(Cmd::SetTimeline(tl)).unwrap();
    tx.send(Cmd::Play).unwrap();
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let mut peak = 0f32;
    for _ in 0..(44100 * 6 / 512) {
        engine.render(&mut l, &mut r);
        for i in 0..512 {
            assert!(l[i].is_finite() && r[i].is_finite());
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
    }
    assert!(peak > 0.02 && peak < 1.5, "peak {peak}");

    // The tremolo bar bends the whole track channel (not only one note).
    let mut found = false;
    for name in ["extra/Effects.gp5", "extra/Effects.gp4", "extra/Effects.gp3", "extra/Wah.gp5", "extra/Demo v5.gp5"] {
        let song = load(name);
        let has_bar = song.tracks.iter().any(|t| t.measures.iter().any(|m| m.beats.iter().any(|b| b.voices.iter().any(|v| v.notes.iter().any(|n| n.effect.tremolo_bar.is_some())))));
        if !has_bar {
            continue;
        }
        let tl = Timeline::build(&song);
        assert!(tl.events.iter().any(|e| matches!(e.kind, Kind::Bend(_)) && tl.channels.contains(&e.channel)), "{name}: no bend on the track channel");
        found = true;
    }
    assert!(found, "no sample with a tremolo bar");
}

/// Optional: set NEOGUITAR_TEST_SF2 to a SoundFont with quirks (e.g. Timbres of Heaven) to check it loads and sounds.
#[test]
fn loads_quirky_soundfont() {
    let Ok(path) = std::env::var("NEOGUITAR_TEST_SF2") else { return };
    let mut file = std::fs::File::open(&path).unwrap();
    let font = Arc::new(SoundFont::new(&mut file).expect("SoundFont should load"));
    let song = load("extra/Chords.gp5");
    let tl = Arc::new(Timeline::build(&song));
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::default());
    shared.mask.store(u64::MAX, Ordering::Relaxed);
    let mut engine = Engine::new(font, 44100, shared, rx).unwrap();
    tx.send(Cmd::SetTimeline(tl)).unwrap();
    tx.send(Cmd::Play).unwrap();
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let mut peak = 0f32;
    for _ in 0..(44100 * 5 / 512) {
        engine.render(&mut l, &mut r);
        peak = l.iter().chain(r.iter()).fold(peak, |p, x| p.max(x.abs()));
    }
    assert!(peak > 0.02, "silent: {peak}");
}
