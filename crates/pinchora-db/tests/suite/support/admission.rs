//! Bound local sockets before a test opens even its PostgreSQL guard connection.
use std::fs::{File, OpenOptions, TryLockError};
use std::time::Duration;

// Shared fixtures use at most 12 concurrent connections each; the 32-connection
// regression and mixed load are exclusive. Three guard sockets plus either 36
// shared fixture sockets or 32 exclusive sockets leave room for the local bot.
const FIXTURE_LANES: i32 = 3;

pub async fn acquire() -> (File, i32) {
    let lanes: Vec<File> = (0..FIXTURE_LANES)
        .map(|lane| {
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(std::env::temp_dir().join(format!("pinchora-db-suite-{lane}.lock")))
                .expect("open fixture admission lock")
        })
        .collect();
    loop {
        for (lane, file) in lanes.iter().enumerate() {
            match file.try_lock() {
                Ok(()) => return (lanes.into_iter().nth(lane).unwrap(), lane as i32),
                Err(TryLockError::WouldBlock) => {}
                Err(error) => panic!("fixture admission lock: {error}"),
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
