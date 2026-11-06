//! Short, read-only virtual-input check. Does not trigger Doubao or play sound.
fn main() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let recorder = doubao_remote::diagnostics::Recorder::start(
        root.path().to_owned(),
        uuid::Uuid::new_v4(),
        48000,
    )?;
    for _ in 0..30 {
        recorder.phone(&[0.0; 960]);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    recorder.finish(true, "capture_probe");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        for dir in std::fs::read_dir(root.path())? {
            let path = dir?.path().join("report.json");
            if let Ok(bytes) = std::fs::read(&path) {
                let report: serde_json::Value = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    report["virtual_capture_error"].is_null(),
                    "{}",
                    report["virtual_capture_error"]
                );
                anyhow::ensure!(
                    report["virtual_total_samples"].as_u64().unwrap_or(0) > 0,
                    "no virtual microphone callbacks"
                );
                println!(
                    "virtual_rate={} virtual_samples={} phone_samples={} capture_error={} dropped={}",
                    report["virtual_rate"],
                    report["virtual_total_samples"],
                    report["phone_total_samples"],
                    report["virtual_capture_error"],
                    report["virtual_samples_dropped"]
                );
                return Ok(());
            }
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "diagnostic save timeout"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
