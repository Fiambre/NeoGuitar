//! Exporta una pista como tablatura de texto plano.

use crate::model::*;

/// Tablatura ASCII de la pista `track`, con líneas de hasta `width` columnas.
pub fn ascii_tab(song: &Song, track: &Track, width: usize) -> String {
    let mut out = String::new();
    let title = if song.title.trim().is_empty() { "(untitled)" } else { song.title.trim() };
    out.push_str(&format!("{title}"));
    if !song.artist.trim().is_empty() {
        out.push_str(&format!(" - {}", song.artist.trim()));
    }
    out.push('\n');
    out.push_str(&format!("Track {}: {}\n", track.number, track.name.trim()));
    if !track.is_percussion {
        let tuning: Vec<String> = track.strings.iter().map(|&m| midi_note_name(m)).collect();
        out.push_str(&format!("Tuning: {}\n", tuning.join(" ")));
    }
    out.push('\n');

    let strings = track.strings.len().max(1);
    let labels: Vec<String> = (0..strings)
        .map(|i| match track.strings.get(i) {
            Some(&m) if !track.is_percussion => midi_note_name(m).trim_end_matches(|c: char| c.is_ascii_digit() || c == '-').to_string(),
            _ => (i + 1).to_string(),
        })
        .collect();
    let label_w = labels.iter().map(|l| l.len()).max().unwrap_or(1);

    // Cada compás es un bloque de texto con una línea por cuerda.
    let mut blocks: Vec<(usize, Vec<String>)> = Vec::new();
    for (mi, measure) in track.measures.iter().enumerate() {
        let mut lines = vec![String::from("-"); strings];
        for beat in &measure.beats {
            let mut cell: Vec<String> = vec![String::new(); strings];
            for voice in beat.voices.iter().filter(|v| !v.empty) {
                for n in &voice.notes {
                    let idx = n.string as usize;
                    if idx == 0 || idx > strings {
                        continue;
                    }
                    cell[idx - 1] = if track.is_percussion {
                        drum_label(n.fret)
                    } else if n.effect.dead {
                        "x".to_string()
                    } else if n.tied || n.effect.ghost {
                        format!("({})", n.fret)
                    } else {
                        n.fret.to_string()
                    };
                }
            }
            let w = cell.iter().map(|c| c.len()).max().unwrap_or(0).max(1) + 1;
            for (s, line) in lines.iter_mut().enumerate() {
                line.push_str(&cell[s]);
                for _ in cell[s].len()..w {
                    line.push('-');
                }
            }
        }
        for line in &mut lines {
            line.push('|');
        }
        blocks.push((mi + 1, lines));
    }

    // Reparto en líneas de texto.
    let mut i = 0;
    while i < blocks.len() {
        let mut used = label_w + 1;
        let mut end = i;
        while end < blocks.len() && (end == i || used + blocks[end].1[0].len() <= width) {
            used += blocks[end].1[0].len();
            end += 1;
        }
        // Números de compás.
        let mut header = " ".repeat(label_w + 1);
        for (number, lines) in &blocks[i..end] {
            let num = number.to_string();
            let w = lines[0].len();
            header.push_str(&num);
            for _ in num.len()..w {
                header.push(' ');
            }
        }
        out.push_str(header.trim_end());
        out.push('\n');
        for s in 0..strings {
            out.push_str(&format!("{:>w$}|", labels[s], w = label_w));
            for (_, lines) in &blocks[i..end] {
                out.push_str(&lines[s]);
            }
            out.push('\n');
        }
        out.push('\n');
        i = end;
    }
    out
}
