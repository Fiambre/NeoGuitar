use neoguitar::model::*;
use neoguitar::playback::*;
use rustysynth::SoundFont;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;

fn load(name: &str) -> Song {
    neoguitar::load(format!("{}/samples/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
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
        if !std::path::Path::new(&format!("{}/samples/{f}", env!("CARGO_MANIFEST_DIR"))).exists() {
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
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", env!("CARGO_MANIFEST_DIR"));
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
    let path = format!("{}/target/breed-render.wav", env!("CARGO_MANIFEST_DIR"));
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
    std::fs::write(format!("{}/target/nothing-else-matters.mid", env!("CARGO_MANIFEST_DIR")), &midi).unwrap();
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
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", env!("CARGO_MANIFEST_DIR"));
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
    std::fs::write(format!("{}/target/{name}", env!("CARGO_MANIFEST_DIR")), wav).unwrap();
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
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", env!("CARGO_MANIFEST_DIR"));
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
    tx.send(Cmd::PlayCountIn { beats: 4, beat_secs: 0.5 }).unwrap();
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
