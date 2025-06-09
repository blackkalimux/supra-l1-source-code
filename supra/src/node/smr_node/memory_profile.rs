use alloc_track::{AllocTrack, BacktraceMode};
use std::alloc::System;
use tokio::task::JoinHandle;
use tracing::info;

#[global_allocator]
static GLOBAL_ALLOC: AllocTrack<System> = AllocTrack::new(System, BacktraceMode::Short);

/// The feature `memory_profile` is only available on Linux because of `procfs` crate which is used by `alloc_track`
pub fn linux_memory_profile() -> JoinHandle<()> {
    tokio::spawn(async move {
        //use profile;
        console_subscriber::init();

        tokio::task::spawn_blocking(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(10000));
            info!("Memeory report start");
            //let report = alloc_track::backtrace_report(|_, _| true);
            //println!("BACKTRACES\n{report}");
            let report = alloc_track::thread_report();
            println!("THREADS\n{report}");
            info!("Memeory report end");
        })
        .await
        .expect("failed to spawn task");
    })
}
