//! Letras externas: lectura de LRC, búsqueda en LRCLIB (lrclib.net) y ajuste de sus tiempos a la tablatura.
//!
//! Los tiempos de una letra vienen de la grabación original y no coinciden con los de la tablatura, así que
//! se llevan al tiempo de la tablatura con una correspondencia lineal por tramos que el usuario ajusta con anclas.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Segundos en el tiempo de la letra (el de la grabación, o uno virtual si la letra no tiene tiempos).
    pub time: f64,
    /// Texto de la línea; vacío para las separaciones entre estrofas.
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// `true` si los tiempos vienen de un LRC; `false` si se repartieron según el largo de cada línea.
    pub timed: bool,
    /// Duración de referencia (segundos) en el tiempo de la letra.
    pub duration: f64,
    /// Origen, para mostrarlo ("LRCLIB: artista - título", nombre del archivo…).
    pub source: String,
}

/// Interpreta un archivo LRC (`[mm:ss.xx]texto`, con varias marcas por línea si hace falta).
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            match parse_stamp(tag) {
                Some(t) => times.push(t),
                None => {
                    // Etiqueta de metadatos ([ar:...], [ti:...]): se descarta la línea completa si no hay marcas.
                    if times.is_empty() {
                        break;
                    }
                }
            }
            rest = stripped[end + 1..].trim_start();
        }
        for t in times {
            out.push(Line { time: t, text: rest.trim().to_string() });
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

fn parse_stamp(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    let m: f64 = m.trim().parse().ok()?;
    let s: f64 = s.trim().replace(',', ".").parse().ok()?;
    (m >= 0.0 && s >= 0.0).then_some(m * 60.0 + s)
}

/// Letra sin tiempos: el tiempo virtual de cada línea es proporcional al texto acumulado.
pub fn from_plain(text: &str, source: String) -> Lyrics {
    let mut lines = Vec::new();
    let mut t = 0.0;
    let mut prev_blank = true;
    for raw in text.lines() {
        let s = raw.trim();
        if s.is_empty() {
            if !prev_blank {
                lines.push(Line { time: t, text: String::new() });
                t += 5.0;
            }
            prev_blank = true;
            continue;
        }
        prev_blank = false;
        lines.push(Line { time: t, text: s.to_string() });
        t += s.chars().count().max(10) as f64;
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    Lyrics { lines, timed: false, duration: t.max(1.0), source }
}

/// Letra a partir del contenido de un archivo: LRC si tiene marcas de tiempo, texto plano si no.
pub fn from_text(text: &str, source: String) -> Lyrics {
    let lines = parse_lrc(text);
    if lines.iter().filter(|l| !l.text.is_empty()).count() >= 2 {
        let duration = lines.last().map(|l| l.time + 6.0).unwrap_or(1.0);
        Lyrics { lines, timed: true, duration, source }
    } else {
        from_plain(text, source)
    }
}

const USER_AGENT: &str = "NeoGuitar (https://github.com/Fiambre/NeoGuitar)";

fn get_json(params: &[(&str, &str)]) -> Result<serde_json::Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(12))).build().into();
    let mut req = agent.get("https://lrclib.net/api/search").header("User-Agent", USER_AGENT);
    for (k, v) in params {
        req = req.query(*k, *v);
    }
    let mut resp = req.call().map_err(|e| format!("could not reach LRCLIB: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("bad response: {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("bad response: {e}"))
}

/// Busca la letra en LRCLIB. `tab_seconds` ayuda a elegir la versión de duración más parecida.
/// Devuelve `Ok(None)` si no hay resultados.
pub fn fetch(artist: &str, title: &str, tab_seconds: f64) -> Result<Option<Lyrics>, String> {
    let (artist, title) = (artist.trim(), title.trim());
    if title.is_empty() {
        return Err("write a title to search for".into());
    }
    let mut results = get_json(&[("track_name", title), ("artist_name", artist)])?;
    if results.as_array().is_none_or(|a| a.is_empty()) {
        let q = format!("{title} {artist}");
        results = get_json(&[("q", q.trim())])?;
    }
    let Some(items) = results.as_array() else { return Ok(None) };

    let mut best: Option<(f64, &serde_json::Value)> = None;
    for item in items {
        if item["instrumental"].as_bool() == Some(true) {
            continue;
        }
        let synced = item["syncedLyrics"].as_str().is_some_and(|s| !s.trim().is_empty());
        let plain = item["plainLyrics"].as_str().is_some_and(|s| !s.trim().is_empty());
        if !synced && !plain {
            continue;
        }
        let dur = item["duration"].as_f64().unwrap_or(0.0);
        // Menor es mejor: con tiempos pesa más que sin ellos; luego, la duración más cercana a la tablatura.
        let score = if synced { 0.0 } else { 1000.0 } + if tab_seconds > 0.0 && dur > 0.0 { (dur - tab_seconds).abs() } else { 0.0 };
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, item));
        }
    }
    let Some((_, item)) = best else { return Ok(None) };
    let name = format!("LRCLIB: {} - {}", item["artistName"].as_str().unwrap_or(artist), item["trackName"].as_str().unwrap_or(title));
    let dur = item["duration"].as_f64().unwrap_or(0.0);
    if let Some(s) = item["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            let last = lines.last().map(|l| l.time + 6.0).unwrap_or(1.0);
            return Ok(Some(Lyrics { lines, timed: true, duration: if dur > 0.0 { dur } else { last }, source: name }));
        }
    }
    let plain = item["plainLyrics"].as_str().unwrap_or("");
    Ok(Some(from_plain(plain, name)))
}

/// Tiempo de la tablatura para cada línea. `anchors` = (tiempo de la letra, tiempo de la tablatura) fijados
/// por el usuario; entre ellos (y los extremos 0 y fin) la correspondencia es lineal.
pub fn tab_times(lyrics: &Lyrics, anchors: &[(f64, f64)], tab_total: f64) -> Vec<f64> {
    let mut pts: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    let mut user = anchors.to_vec();
    user.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.extend(user);
    pts.push((lyrics.duration.max(pts.last().map(|p| p.0).unwrap_or(0.0) + 1.0), tab_total.max(pts.last().map(|p| p.1).unwrap_or(0.0) + 1.0)));
    // Mantiene el orden en ambos ejes para que la correspondencia sea monótona.
    let mut mono: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for p in pts {
        match mono.last() {
            Some(&(lt, tt)) if p.0 <= lt || p.1 < tt => {
                if p.0 > lt {
                    mono.push((p.0, tt));
                }
            }
            _ => mono.push(p),
        }
    }
    lyrics
        .lines
        .iter()
        .map(|l| {
            let i = mono.partition_point(|p| p.0 <= l.time).clamp(1, mono.len() - 1);
            let (a, b) = (mono[i - 1], mono[i]);
            let f = if b.0 > a.0 { (l.time - a.0) / (b.0 - a.0) } else { 0.0 };
            a.1 + (b.1 - a.1) * f
        })
        .collect()
}

/// Línea (no vacía) que corresponde al instante `t` de la tablatura.
pub fn current(lyrics: &Lyrics, times: &[f64], t: f64) -> Option<usize> {
    let i = times.partition_point(|&x| x <= t);
    (0..i).rev().find(|&k| !lyrics.lines[k].text.is_empty())
}

/// Siguiente línea no vacía después de `from`.
pub fn next_line(lyrics: &Lyrics, from: usize) -> Option<usize> {
    (from + 1..lyrics.lines.len()).find(|&k| !lyrics.lines[k].text.is_empty())
}

// ------------------------------------------------------------------ caché local

fn cache_file(dir: &Path, artist: &str, title: &str) -> PathBuf {
    let clean: String = format!("{artist} - {title}").chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    dir.join(format!("{}.txt", clean.trim().to_lowercase()))
}

pub fn cache_load(dir: &Path, artist: &str, title: &str) -> Option<Lyrics> {
    let text = std::fs::read_to_string(cache_file(dir, artist, title)).ok()?;
    let (head, body) = text.split_once("\n---\n")?;
    let mut l = from_text(body, String::new());
    for line in head.lines() {
        if let Some(v) = line.strip_prefix("source=") {
            l.source = format!("{v} (saved)");
        }
        if let (Some(v), true) = (line.strip_prefix("duration="), l.timed) {
            if let Ok(d) = v.parse::<f64>() {
                if d > 0.0 {
                    l.duration = d;
                }
            }
        }
    }
    (!l.lines.is_empty()).then_some(l)
}

pub fn cache_store(dir: &Path, artist: &str, title: &str, l: &Lyrics) {
    let body: String = l
        .lines
        .iter()
        .map(|x| if l.timed { format!("[{:02}:{:05.2}]{}\n", (x.time / 60.0) as u32, x.time % 60.0, x.text) } else { format!("{}\n", x.text) })
        .collect();
    let text = format!("source={}\nduration={}\n---\n{}", l.source, l.duration, body);
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(cache_file(dir, artist, title), text);
}
