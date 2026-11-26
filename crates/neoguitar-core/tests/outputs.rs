use neoguitar_core::audio::{output_devices, Outputs, Player, Status};
use std::time::Duration;

/// Needs sound hardware: cargo test --release --test outputs -- --ignored --nocapture
#[test]
#[ignore]
fn lists_devices_and_opens_a_separate_metronome_output() {
    let devices = output_devices();
    println!("devices: {devices:?}");
    assert!(!devices.is_empty(), "no output devices");
    let sf = format!("{}/soundfonts/GeneralUser-GS.sf2", concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut p = Player::new(Some(sf.into()));
    // Both outputs on the same device, the metronome on another channel pair.
    p.outputs = Outputs { main: None, click: Some(devices[0].clone()), click_channel: 0, offset_ms: 20 };
    for _ in 0..200 {
        p.poll();
        if !matches!(p.status, Status::Loading(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("status ready: {}", p.ready());
    assert!(p.ready());
    std::thread::sleep(Duration::from_millis(300));
    // Back to a single output.
    p.set_outputs(Outputs::default());
    assert!(p.ready());
}
