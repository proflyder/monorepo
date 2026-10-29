//! Temporary standalone capture process, never an assistant or a daemon replacement on disk.
#![forbid(unsafe_code)]
use pulsar_audio::FrontendEvent;
use std::io::{Write, stdout};
use std::path::PathBuf;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .filter(|s| (5..=600).contains(s))
        .expect("duration must be 5..600 seconds");
    // Independent of the async loop: also bounds blocked stdout / VQE / teardown.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        std::process::exit(0);
    });
    pulsar_core::init_logger(); // stderr only
    let model = pulsar_core::model::detect_model().expect("unsupported station");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    rt.block_on(async {
        let (front, mut events) = pulsar_audio::spawn(
            model,
            PathBuf::from("/system/vendor/quasar/spotter_models/activation/alisa"),
            None,
            false,
            None,
        );
        front.set_detection(false);
        front.set_commands(false);
        front.set_preroll(false);
        front.set_music_analysis(false);
        front.set_streaming(true);
        let mut out = stdout().lock();
        let mut chunks = 0;
        let mut ready = false;
        loop {
            let event = tokio::time::timeout(Duration::from_secs(12), events.recv())
                .await
                .map_err(|_| std::io::Error::other("microphone produced no audio"))?;
            match event {
                Some(FrontendEvent::Audio(samples)) => {
                    chunks += 1;
                    // Let VQE settle; never include startup noise in a labelled take.
                    if chunks < 32 {
                        continue;
                    }
                    if !ready {
                        out.write_all(b"RICARDO_PCM_S16LE_16000\n")?;
                        ready = true;
                    }
                    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                    // SSH -T is an unmodified binary channel, unlike the legacy ADB PTY.
                    out.write_all(&bytes)?;
                    out.flush()?;
                }
                Some(FrontendEvent::Wake { .. }) => front.wake_handled(),
                None => return Err(std::io::Error::other("audio stream ended")),
                _ => {}
            }
        }
    })
}
