#[test]
fn settings_roundtrip() {
    use neoguitar_win::settings::Settings;
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
