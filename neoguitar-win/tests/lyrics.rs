use neoguitar::lyrics::*;

#[test]
fn parses_lrc_with_tags_and_multiple_stamps() {
    let lines = parse_lrc("[ar:Someone]\n[ti:Song]\n[00:12.50]First\n[00:05.00][01:00.25]Chorus\n[00:30.00]\nnot a line\n");
    let got: Vec<(String, String)> = lines.iter().map(|l| (format!("{:.2}", l.time), l.text.clone())).collect();
    assert_eq!(got, vec![("5.00".into(), "Chorus".into()), ("12.50".into(), "First".into()), ("30.00".into(), "".into()), ("60.25".into(), "Chorus".into())]);
}

#[test]
fn plain_lyrics_keep_stanza_breaks() {
    let l = from_plain("one two three\nfour\n\n\nfive six\n", "test".into());
    let texts: Vec<&str> = l.lines.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(texts, vec!["one two three", "four", "", "five six"]);
    assert!(!l.timed && l.duration > 0.0);
    assert!(l.lines.windows(2).all(|w| w[0].time < w[1].time));
}

#[test]
fn mapping_scales_to_the_tab_and_follows_anchors() {
    let l = from_text("[00:00.00]a\n[00:20.00]b\n[00:40.00]c\n[01:00.00]d\n", "t".into());
    assert!(l.timed);
    // Without anchors the lyric timeline is stretched over the tab length.
    let t = tab_times(&l, &[], 120.0);
    assert!(t[0].abs() < 1e-9);
    assert!(t.windows(2).all(|w| w[0] < w[1]));
    assert!(t[3] > t[1] * 2.0);
    // Pin line "c" (40 s in the recording) to 50 s of the tab: it lands there and the order is kept.
    let t = tab_times(&l, &[(40.0, 50.0)], 120.0);
    assert!((t[2] - 50.0).abs() < 1e-6, "{t:?}");
    assert!(t.windows(2).all(|w| w[0] <= w[1]));
    // The active line follows the playback time.
    assert_eq!(current(&l, &t, 0.5), Some(0));
    assert_eq!(current(&l, &t, 51.0), Some(2));
    assert_eq!(next_line(&l, 2), Some(3));
}

#[test]
fn cache_roundtrip() {
    let dir = std::env::temp_dir().join(format!("neoguitar-lyrics-{}", std::process::id()));
    let l = from_text("[00:01.00]hello\n[00:05.50]world\n", "src".into());
    cache_store(&dir, "Art/ist", "Ti:tle", &l);
    let back = cache_load(&dir, "Art/ist", "Ti:tle").expect("cached");
    assert_eq!(back.lines.len(), 2);
    assert!(back.timed);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Needs internet: cargo test --release --test lyrics -- --ignored
#[test]
#[ignore]
fn lrclib_lookup() {
    let l = fetch("Feel Trip", "Mi buena suerte", 170.0).expect("request").expect("a result");
    println!("{} | timed={} | {} lines | {:.0}s", l.source, l.timed, l.lines.len(), l.duration);
    assert!(l.lines.len() > 5);
}
