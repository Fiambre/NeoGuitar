//! Salida de audio (cpal) y carga del SoundFont en segundo plano.

use crate::model::Song;
use crate::playback::{Cmd, Engine, Shared, Timeline, MIX_FILE};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustysynth::SoundFont;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub enum Status {
    /// No se encontró ningún SoundFont.
    NoSoundFont,
    Loading(PathBuf),
    Ready(PathBuf),
    Error(String),
}

pub struct Player {
    pub status: Status,
    pub shared: Arc<Shared>,
    pub timeline: Arc<Timeline>,
    /// Línea de tiempo sin transponer, de la que sale `timeline`.
    base_timeline: Arc<Timeline>,
    /// Transposición del sonido en semitonos (la tablatura no cambia).
    pub transpose: i32,
    tx: Sender<Cmd>,
    loader: Option<Receiver<Result<Arc<SoundFont>, String>>>,
    stream: Option<cpal::Stream>,
    /// SoundFont ya cargado (para exportar audio sin pasar por el dispositivo).
    font: Option<Arc<SoundFont>>,
    pub speed: f32,
    pub volume: f32,
    /// Compases (desde, hasta) del bucle, base 0, ambos inclusive.
    pub loop_measures: Option<(usize, usize)>,
}

impl Player {
    /// `soundfont`: el SoundFont guardado en los ajustes, si existe; si no se busca uno.
    pub fn new(soundfont: Option<PathBuf>) -> Player {
        let (tx, _rx) = channel();
        let shared = Arc::new(Shared::default());
        shared.mask.store(u64::MAX, Ordering::Relaxed);
        let mut p = Player {
            status: Status::NoSoundFont,
            shared,
            timeline: Arc::new(Timeline::default()),
            base_timeline: Arc::new(Timeline::default()),
            transpose: 0,
            tx,
            loader: None,
            stream: None,
            font: None,
            speed: 1.0,
            volume: 1.0,
            loop_measures: None,
        };
        let chosen = soundfont.filter(|p| p.is_file()).or_else(find_soundfont);
        if let Some(path) = chosen {
            p.load_soundfont(&path);
        }
        p
    }

    pub fn load_soundfont(&mut self, path: &Path) {
        self.stream = None; // detiene el motor anterior
        self.font = None;
        self.shared.playing.store(false, Ordering::Relaxed);
        self.status = Status::Loading(path.to_path_buf());
        let (tx, rx) = channel();
        self.loader = Some(rx);
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let result = std::fs::File::open(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .and_then(|mut f| SoundFont::new(&mut f).map_err(|e| format!("Invalid SoundFont: {e}")))
                .map(Arc::new);
            let _ = tx.send(result);
        });
    }

    /// Debe llamarse en cada frame: completa la carga del SoundFont y abre el dispositivo.
    pub fn poll(&mut self) {
        let Some(loader) = &self.loader else { return };
        let Ok(result) = loader.try_recv() else { return };
        self.loader = None;
        let Status::Loading(path) = std::mem::replace(&mut self.status, Status::NoSoundFont) else { return };
        match result.and_then(|font| self.start_stream(font)) {
            Ok(()) => self.status = Status::Ready(path),
            Err(e) => self.status = Status::Error(e),
        }
    }

    fn start_stream(&mut self, font: Arc<SoundFont>) -> Result<(), String> {
        self.font = Some(font.clone());
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let config = device.default_output_config().map_err(|e| format!("audio configuration: {e}"))?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!("unsupported audio format: {:?}", config.sample_format()));
        }
        let channels = config.channels() as usize;
        let sample_rate: u32 = config.sample_rate().into();

        // Se crea un canal nuevo para el motor nuevo y se reenvía el estado actual.
        let (tx, rx) = channel();
        let mut engine = Engine::new(font, sample_rate, self.shared.clone(), rx)?;
        let _ = tx.send(Cmd::SetTimeline(self.timeline.clone()));
        let _ = tx.send(Cmd::SetSpeed(self.speed));
        let _ = tx.send(Cmd::SetVolume(self.volume));
        self.tx = tx;

        let (mut left, mut right) = (Vec::<f32>::new(), Vec::<f32>::new());
        let stream = device
            .build_output_stream(
                config.into(),
                move |data: &mut [f32], _| {
                    let frames = data.len() / channels.max(1);
                    left.resize(frames, 0.0);
                    right.resize(frames, 0.0);
                    engine.render(&mut left, &mut right);
                    for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                        frame[0] = left[i];
                        if channels > 1 {
                            frame[1] = right[i];
                        }
                        for extra in frame.iter_mut().skip(2) {
                            *extra = 0.0;
                        }
                    }
                },
                |e| eprintln!("audio error: {e}"),
                None,
            )
            .map_err(|e| format!("could not open audio: {e}"))?;
        stream.play().map_err(|e| format!("could not start audio: {e}"))?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn ready(&self) -> bool {
        matches!(self.status, Status::Ready(_))
    }

    pub fn set_song(&mut self, song: &Song) {
        self.base_timeline = Arc::new(Timeline::build(song));
        self.timeline = self.base_timeline.clone();
        self.transpose = 0;
        self.shared.mask.store(u64::MAX, Ordering::Relaxed);
        for g in &self.shared.gains {
            g.store(100, Ordering::Relaxed);
        }
        for a in self.shared.mix_pan.iter().chain(&self.shared.mix_program).chain(&self.shared.mix_reverb) {
            a.store(MIX_FILE, Ordering::Relaxed);
        }
        self.clear_loop();
        self.send(Cmd::SetTimeline(self.timeline.clone()));
    }

    /// Transpone el sonido `semitones` semitonos (-12..12) sin interrumpir la reproducción.
    pub fn set_transpose(&mut self, semitones: i32) {
        let n = semitones.clamp(-12, 12);
        if n == self.transpose {
            return;
        }
        self.transpose = n;
        self.timeline = if n == 0 { self.base_timeline.clone() } else { Arc::new(self.base_timeline.transposed(n)) };
        self.send(Cmd::ReplaceTimeline(self.timeline.clone()));
    }

    pub fn play(&self) {
        self.send(Cmd::Play);
    }

    /// Como `play`, pero con una cuenta regresiva de un compás antes de empezar.
    pub fn play_with_count_in(&self) {
        match self.timeline.count_in(self.position()) {
            Some((beats, beat_secs)) => self.send(Cmd::PlayCountIn { beats, beat_secs }),
            None => self.send(Cmd::Play),
        }
    }

    pub fn trainer(&self) -> (bool, u32, u32) {
        (
            self.shared.trainer_on.load(Ordering::Relaxed),
            self.shared.trainer_step.load(Ordering::Relaxed),
            self.shared.trainer_max.load(Ordering::Relaxed),
        )
    }

    pub fn set_trainer(&self, on: bool, step: u32, max: u32) {
        self.shared.trainer_step.store(step, Ordering::Relaxed);
        self.shared.trainer_max.store(max, Ordering::Relaxed);
        self.shared.trainer_on.store(on, Ordering::Relaxed);
    }

    /// Toma del motor la velocidad vigente (la cambia el entrenador de velocidad).
    pub fn sync_speed(&mut self) {
        let pct = self.shared.speed_pct.load(Ordering::Relaxed) as f32 / 100.0;
        if (pct - self.speed).abs() > 0.004 {
            self.speed = pct;
        }
    }

    pub fn pause(&self) {
        self.send(Cmd::Pause);
    }

    pub fn stop(&self) {
        self.send(Cmd::Stop);
    }

    pub fn seek(&self, seconds: f64) {
        self.send(Cmd::Seek(seconds));
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
        self.shared.speed_pct.store((speed * 100.0).round() as u32, Ordering::Relaxed);
        self.send(Cmd::SetSpeed(speed));
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        self.send(Cmd::SetVolume(volume));
    }

    pub fn metronome(&self) -> bool {
        self.shared.metronome.load(Ordering::Relaxed)
    }

    pub fn set_metronome(&self, on: bool) {
        self.shared.metronome.store(on, Ordering::Relaxed);
    }

    pub fn loop_enabled(&self) -> bool {
        self.shared.loop_on.load(Ordering::Relaxed)
    }

    pub fn set_loop_enabled(&self, on: bool) {
        self.shared.loop_on.store(on && self.loop_measures.is_some(), Ordering::Relaxed);
    }

    /// Define el bucle entre dos compases (en cualquier orden) y lo activa.
    pub fn set_loop(&mut self, a: usize, b: usize) {
        let (from, to) = (a.min(b), a.max(b));
        let (Some(t0), Some(t1)) = (self.timeline.time_of_measure(from), self.timeline.first_end_of_measure(to)) else {
            return;
        };
        self.shared.set_loop(t0, t1);
        self.loop_measures = Some((from, to));
        self.shared.loop_on.store(true, Ordering::Relaxed);
    }

    pub fn clear_loop(&mut self) {
        self.loop_measures = None;
        self.shared.loop_on.store(false, Ordering::Relaxed);
    }

    /// Guarda la canción como archivo MIDI estándar con las pistas activas.
    pub fn export_midi(&self, path: &Path) -> Result<(), String> {
        let mask = self.shared.mask.load(Ordering::Relaxed);
        let bytes = self.timeline.to_midi(mask, self.metronome());
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Renderiza la canción completa a un WAV (16 bits, estéreo, 44,1 kHz) en un hilo aparte.
    /// Respeta las pistas activas, sus ganancias, el metrónomo y el volumen actuales; siempre a velocidad 100 %.
    pub fn export_wav(&self, path: &Path) -> Result<Receiver<Result<(), String>>, String> {
        let font = self.font.clone().ok_or("the SoundFont is not loaded yet")?;
        let timeline = self.timeline.clone();
        if timeline.total <= 0.0 {
            return Err("the song has nothing to export".into());
        }
        let mask = self.shared.mask.load(Ordering::Relaxed);
        let gains: Vec<u8> = self.shared.gains.iter().map(|g| g.load(Ordering::Relaxed)).collect();
        let (metronome, volume) = (self.metronome(), self.volume);
        let path = path.to_path_buf();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(render_wav(font, timeline, mask, &gains, metronome, volume, &path));
        });
        Ok(rx)
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    pub fn position(&self) -> f64 {
        self.shared.position()
    }

    pub fn track_enabled(&self, track: usize) -> bool {
        track >= 64 || self.shared.mask.load(Ordering::Relaxed) & (1u64 << track) != 0
    }

    /// Ganancia de la pista en porcentaje (0..=200).
    pub fn track_gain(&self, track: usize) -> u8 {
        self.shared.gains.get(track).map(|g| g.load(Ordering::Relaxed)).unwrap_or(100)
    }

    pub fn set_track_gain(&self, track: usize, percent: u8) {
        if let Some(g) = self.shared.gains.get(track) {
            g.store(percent, Ordering::Relaxed);
        }
    }

    /// Deja sonar solo `track`; si ya era la única activa, vuelven todas.
    /// Paneo (0..=127) cambiado por el usuario, si lo hay.
    pub fn mix_pan(&self, track: usize) -> Option<u8> {
        self.mix_value(&self.shared.mix_pan, track)
    }

    /// Instrumento (0..=127) cambiado por el usuario, si lo hay.
    pub fn mix_program(&self, track: usize) -> Option<u8> {
        self.mix_value(&self.shared.mix_program, track)
    }

    /// Envío de reverb (0..=127) cambiado por el usuario, si lo hay.
    pub fn mix_reverb(&self, track: usize) -> Option<u8> {
        self.mix_value(&self.shared.mix_reverb, track)
    }

    fn mix_value(&self, list: &[std::sync::atomic::AtomicU8], track: usize) -> Option<u8> {
        list.get(track).map(|a| a.load(Ordering::Relaxed)).filter(|&v| v != MIX_FILE)
    }

    pub fn set_mix(&self, track: usize, pan: Option<u8>, program: Option<u8>, reverb: Option<u8>) {
        let put = |list: &[std::sync::atomic::AtomicU8], v: Option<u8>| {
            if let Some(a) = list.get(track) {
                a.store(v.map(|x| x.min(127)).unwrap_or(MIX_FILE), Ordering::Relaxed);
            }
        };
        put(&self.shared.mix_pan, pan);
        put(&self.shared.mix_program, program);
        put(&self.shared.mix_reverb, reverb);
    }

    /// Deja sonar solo `track`; si ya era la única activa, vuelven todas.
    pub fn toggle_solo(&self, track: usize, total: usize) {
        let only_this = (0..total).all(|i| self.track_enabled(i) == (i == track));
        for i in 0..total {
            self.set_track_enabled(i, only_this || i == track);
        }
    }

    pub fn set_track_enabled(&self, track: usize, enabled: bool) {
        if track < 64 {
            if enabled {
                self.shared.mask.fetch_or(1u64 << track, Ordering::Relaxed);
            } else {
                self.shared.mask.fetch_and(!(1u64 << track), Ordering::Relaxed);
            }
        }
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

/// Busca un SoundFont: variable NEOGUITAR_SOUNDFONT o la carpeta `soundfonts/` junto al
/// ejecutable, en el directorio actual o en algún directorio superior del ejecutable.
pub fn find_soundfont() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NEOGUITAR_SOUNDFONT") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..4 {
            let Some(cur) = d else { break };
            dirs.push(cur.join("soundfonts"));
            d = cur.parent().map(|p| p.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("soundfonts"));
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sf2")))
            .collect();
        found.sort();
        if let Some(p) = found.into_iter().next() {
            return Some(p);
        }
    }
    None
}

/// Render fuera de línea de toda la canción a un archivo WAV.
pub fn render_wav(font: Arc<SoundFont>, timeline: Arc<Timeline>, mask: u64, gains: &[u8], metronome: bool, volume: f32, path: &Path) -> Result<(), String> {
    const SAMPLE_RATE: u32 = 44100;
    const CHUNK: usize = 512;
    let shared = Arc::new(Shared::default());
    shared.mask.store(mask, Ordering::Relaxed);
    shared.metronome.store(metronome, Ordering::Relaxed);
    for (g, v) in shared.gains.iter().zip(gains) {
        g.store(*v, Ordering::Relaxed);
    }
    let total = timeline.total;
    let (tx, rx) = channel();
    let mut engine = Engine::new(font, SAMPLE_RATE, shared.clone(), rx)?;
    let _ = tx.send(Cmd::SetTimeline(timeline));
    let _ = tx.send(Cmd::SetVolume(volume));
    let _ = tx.send(Cmd::Play);

    let max_frames = ((total + 10.0) * SAMPLE_RATE as f64) as usize;
    let mut pcm: Vec<f32> = Vec::with_capacity(((total + 2.0) * SAMPLE_RATE as f64) as usize * 2);
    let (mut l, mut r) = (vec![0f32; CHUNK], vec![0f32; CHUNK]);
    let mut peak = 0f32;
    while pcm.len() / 2 < max_frames {
        engine.render(&mut l, &mut r);
        for i in 0..CHUNK {
            peak = peak.max(l[i].abs()).max(r[i].abs());
            pcm.push(l[i]);
            pcm.push(r[i]);
        }
        if !shared.playing.load(Ordering::Relaxed) {
            break;
        }
    }
    // Si se pasa de 0 dBFS, se baja el nivel en vez de recortar.
    let scale = if peak > 0.99 { 0.99 / peak } else { 1.0 };
    let data_len = (pcm.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&2u16.to_le_bytes()); // canales
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for v in &pcm {
        wav.extend_from_slice(&(((v * scale).clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, wav).map_err(|e| format!("{}: {e}", path.display()))
}
