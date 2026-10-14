//! Bounded CPU load used to simulate a colocated batch job.

use std::hint::black_box;
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(about = "Simulate a CPU-heavy process sharing the app's host")]
struct Cli {
    /// Busy worker threads to run
    #[arg(long, default_value_t = 4)]
    threads: usize,

    /// How long to generate load
    #[arg(long, default_value_t = 30)]
    duration: u64,
}

fn main() {
    let cli = Cli::parse();
    let deadline = Instant::now() + Duration::from_secs(cli.duration);

    println!(
        "cpu-noise: running {} busy threads for {}s",
        cli.threads, cli.duration
    );

    let workers: Vec<_> = (0..cli.threads)
        .map(|worker| {
            thread::spawn(move || {
                let mut value = worker as u64 + 1;
                while Instant::now() < deadline {
                    for _ in 0..10_000 {
                        value = value
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1);
                    }
                    black_box(value);
                }
            })
        })
        .collect();

    for worker in workers {
        worker.join().expect("CPU worker panicked");
    }

    println!("cpu-noise: done");
}
