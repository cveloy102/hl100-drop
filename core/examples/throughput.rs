//! Compare identical raw TCP transfers with and without resume negotiation.
use hl100_core::{collect_paths, Config, DiskStorage, Engine};
use std::{
    fs::{self, File},
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener},
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
fn port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("own benchmark directory"));
    fs::create_dir_all(root.join("received")).unwrap();
    let source = root.join("throughput.bin");
    let block = vec![0x5a; 1024 * 1024];
    let size = 256 * 1024 * 1024u64;
    {
        let mut file = File::create(&source).unwrap();
        for _ in 0..256 {
            file.write_all(&block).unwrap();
        }
    }
    let receiver_port = port();
    let receiver = Engine::start(
        Config {
            id: "benchmark-r".into(),
            name: "Benchmark receiver".into(),
            folder: root.join("received").to_string_lossy().into(),
            port: receiver_port,
        },
        Arc::new(DiskStorage::new(root.join("received"))),
    )
    .unwrap();
    let sender = Engine::start(
        Config {
            id: "benchmark-s".into(),
            name: "Benchmark sender".into(),
            folder: root.join("sender").to_string_lossy().into(),
            port: port(),
        },
        Arc::new(DiskStorage::new(root.join("sender"))),
    )
    .unwrap();
    for round in 0..6 {
        let resumable = round % 2 == 1;
        let mut sources = collect_paths(&[source.to_string_lossy().into()]).unwrap();
        if !resumable {
            for source in &mut sources {
                source.locator = None;
            }
        }
        let start = Instant::now();
        sender
            .send(
                (Ipv4Addr::LOCALHOST, receiver_port).into(),
                "Benchmark receiver".into(),
                sources,
            )
            .unwrap();
        let deadline = start + Duration::from_secs(60);
        let mut complete = false;
        while Instant::now() < deadline {
            let state = sender.poll();
            if state["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["type"] == "complete" && e["status"] == "done")
            {
                complete = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(complete, "transfer did not complete");
        let elapsed = start.elapsed().as_secs_f64();
        let output = fs::read_dir(root.join("received"))
            .unwrap()
            .flatten()
            .find(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("bin"))
            .unwrap()
            .path();
        let mut file = File::open(&output).unwrap();
        let mut verify = vec![0; block.len()];
        for _ in 0..256 {
            file.read_exact(&mut verify).unwrap();
            assert_eq!(verify, block);
        }
        assert_eq!(file.metadata().unwrap().len(), size);
        drop(file);
        fs::remove_file(output).unwrap();
        println!(
            "{} {:.3} seconds {:.1} MiB/s",
            if resumable { "resume" } else { "legacy" },
            elapsed,
            256.0 / elapsed
        );
    }
    sender.stop();
    receiver.stop();
    fs::remove_file(source).unwrap();
}
