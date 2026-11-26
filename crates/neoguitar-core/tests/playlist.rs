use neoguitar_core::playlist::*;
use std::path::{Path, PathBuf};

#[test]
fn roundtrip_with_relative_paths() {
    let base = Path::new("C:/Music/band");
    let mut a = Entry::new(base.join("songs/one.gp5"));
    a.mask = 0b1011;
    a.gains = vec![(1, 60), (3, 120)];
    a.speed = 80;
    a.metronome = true;
    a.count_in = true;
    a.loop_measures = Some((4, 8));
    a.transpose = -2;
    a.note = "only verses".into();
    let b = Entry::new(PathBuf::from("D:/elsewhere/two.gp4"));
    let pl = Playlist { name: "Rehearsal".into(), entries: vec![a, b] };

    let text = pl.serialize(base);
    assert!(text.contains("file=songs") && text.contains("one.gp5"), "{text}");
    assert!(!text.contains("C:/Music"), "inside the playlist folder the path is relative: {text}");
    assert!(text.contains("D:/elsewhere") || text.contains("D:\\elsewhere"), "outside it stays absolute");

    // The same file read from another folder keeps the songs next to it.
    let moved = Path::new("E:/other-pc/band");
    let back = Playlist::parse(&text, moved);
    assert_eq!(back.name, "Rehearsal");
    assert_eq!(back.entries.len(), 2);
    let e = &back.entries[0];
    assert_eq!(e.path, moved.join("songs/one.gp5"));
    assert_eq!((e.mask, e.speed, e.metronome, e.count_in, e.loop_measures, e.transpose), (0b1011, 80, true, true, Some((4, 8)), -2));
    assert_eq!(e.gains, vec![(1, 60), (3, 120)]);
    assert_eq!(e.note, "only verses");
    assert_eq!(back.entries[1].mask, u64::MAX);
}

#[test]
fn ignores_garbage_and_clamps() {
    let pl = Playlist::parse("name=x\nnoise\n[song]\nfile=a.gp5\nspeed=9999\ngain=99:10\ngain=2:300\ntranspose=40\n[song]\nspeed=50\n", Path::new("."));
    assert_eq!(pl.entries.len(), 1, "an entry without a file is dropped");
    let e = &pl.entries[0];
    assert_eq!((e.speed, e.transpose), (150, 12));
    assert!(e.gains.is_empty());
}

#[test]
fn save_and_load_file() {
    let dir = std::env::temp_dir().join(format!("neoguitar-playlist-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("set.ngpl");
    let mut e = Entry::new(dir.join("a.gp5"));
    e.mask = 0b10;
    Playlist { name: "set".into(), entries: vec![e] }.save(&file).unwrap();
    let back = Playlist::load(&file).unwrap();
    assert_eq!(back.entries[0].path, dir.join("a.gp5"));
    assert_eq!(back.entries[0].mask, 0b10);
    let _ = std::fs::remove_dir_all(&dir);
}
