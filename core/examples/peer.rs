//! Local QA peer. Never runs unless explicitly launched.
use hl100_core::{collect_paths, Config, DiskStorage, Engine};
use std::{sync::Arc, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let folder = args.get(1).ok_or("usage: peer FOLDER [ADDRESS FILE]")?;
    let engine = Engine::start(
        Config {
            id: "qa-peer".into(),
            name: "QA peer".into(),
            folder: folder.clone(),
            port: 41002,
        },
        Arc::new(DiskStorage::new(folder)),
    )?;
    if args.len() >= 4 {
        engine.send(
            args[2].parse()?,
            "QA recipient".into(),
            collect_paths(&[args[3].clone()])?,
        )?;
    }
    for _ in 0..240 {
        let state = engine.poll();
        if !state["events"].as_array().unwrap().is_empty() {
            println!("{state}");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    engine.stop();
    Ok(())
}
