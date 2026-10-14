//! A small storefront used in the async observability workshop.

mod app;
mod background;
mod model;
mod routes;
mod services;
mod state;

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use dial9::{Dial9HandleTokioExt, DiskBuffer, TokioAttachOptions};
use tracing_subscriber::EnvFilter;

const TRACE_BUDGET: u64 = 512 * 1024 * 1024;
const TRACE_ROTATION_PERIOD: Duration = Duration::from_secs(60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const SERVICE: &str = "workshop-app";
const DEFAULT_INSTANCE: &str = "local";
const LOG_FILTER: &str = "info,workshop_app=debug";

#[derive(Parser, Debug)]
#[command(about = "The service used in the async observability workshop")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,

    /// Parent directory in which this run's trace is created
    #[arg(long, default_value = "traces/local")]
    trace_dir: PathBuf,

    /// Dial9 viewer used for the direct trace link printed on shutdown
    #[arg(long, default_value = "http://localhost:3001")]
    viewer_url: String,

    /// Product catalog loaded by the service
    #[arg(long, default_value_os_t = default_catalog_path())]
    catalog: PathBuf,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let trace_dir = new_trace_dir(&cli.trace_dir)?;
    let instance = std::env::var("WORKSHOP_INSTANCE").unwrap_or_else(|_| DEFAULT_INSTANCE.into());
    let log_file = std::fs::File::create(trace_dir.join("workshop.log"))?;
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::new(LOG_FILTER))
        .with_target(false)
        .with_writer(std::sync::Mutex::new(log_file))
        .try_init()
        .map_err(io::Error::other)?;

    let buffer = DiskBuffer::builder()
        .base_path(&trace_dir)
        .max_total_size(TRACE_BUDGET)
        .rotation_period(TRACE_ROTATION_PERIOD)
        .build()?;
    let recorder = dial9::recorder(buffer).segment_metadata([
        ("service".to_string(), SERVICE.to_string()),
        ("host".to_string(), instance),
    ]);
    #[cfg(feature = "linux-profiling")]
    let recorder = {
        use dial9::RecorderPerfExt as _;

        recorder.with_cpu_profiling(dial9::cpu::CpuProfilingConfig::default())
    };
    let recorder = recorder.build();

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.worker_threads(4).enable_all();
    let runtime = recorder.handle().attach_tokio_runtime(
        builder,
        TokioAttachOptions::builder()
            .runtime_name(SERVICE)
            .task_tracking_enabled(true)
            .build(),
    )?;

    println!("writing Dial9 trace to {}", trace_dir.display());
    let result = runtime.block_on(app::serve(cli.listen, cli.catalog));

    // Runtime workers flush their local trace buffers as they exit.
    runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
    recorder.graceful_shutdown(SHUTDOWN_TIMEOUT);
    println!("trace sealed in {}", trace_dir.display());

    let viewer_root = cli.trace_dir.parent().unwrap_or(&cli.trace_dir);
    match direct_viewer_link(&cli.viewer_url, viewer_root, &trace_dir)? {
        Some(link) => println!("open the trace in Dial9:\n{link}"),
        None => eprintln!("no sealed trace segments found in {}", trace_dir.display()),
    }
    result
}

fn default_catalog_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/products.json")
}

fn new_trace_dir(root: &Path) -> io::Result<PathBuf> {
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = root.join(format!("run-{started_at}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn direct_viewer_link(base_url: &str, root: &Path, dir: &Path) -> io::Result<Option<String>> {
    let mut keys = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !(name.ends_with(".bin") || name.ends_with(".bin.gz")) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(io::Error::other)?
            .to_string_lossy()
            .replace('\\', "/");
        keys.push(relative);
    }
    keys.sort();
    if keys.is_empty() {
        return Ok(None);
    }

    let mut link = format!("{}/viewer.html?", base_url.trim_end_matches('/'));
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            link.push('&');
        }
        let object_url = format!("/api/object?key={}", percent_encode(key));
        link.push_str("trace=");
        link.push_str(&percent_encode(&object_url));
    }
    Ok(Some(link))
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_directory_is_unique_per_run() {
        let root =
            std::env::temp_dir().join(format!("workshop-app-layout-test-{}", std::process::id()));
        let dir = new_trace_dir(&root).unwrap();
        let relative = dir.strip_prefix(&root).unwrap().to_string_lossy();

        assert!(relative.starts_with("run-"));
        assert_eq!(relative.matches('/').count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn viewer_link_opens_the_sealed_segments_directly() {
        let root =
            std::env::temp_dir().join(format!("workshop-app-link-test-{}", std::process::id()));
        let run = root.join("local/run-1");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join("trace.0.bin"), b"first").unwrap();
        std::fs::write(run.join("trace.1.bin.gz"), b"second").unwrap();
        std::fs::write(run.join("trace.2.bin.active"), b"active").unwrap();

        let link = direct_viewer_link("http://localhost:3001/", &root, &run)
            .unwrap()
            .unwrap();
        assert_eq!(
            link,
            "http://localhost:3001/viewer.html?trace=%2Fapi%2Fobject%3Fkey%3Dlocal%252Frun-1%252Ftrace.0.bin&trace=%2Fapi%2Fobject%3Fkey%3Dlocal%252Frun-1%252Ftrace.1.bin.gz"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
