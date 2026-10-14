//! Small open-loop HTTP load generator for the workshop application.
//!
//! This process owns no server or Dial9 setup. Keeping it outside the
//! application runtime means a stalled server cannot also stall the client
//! measuring it.

mod load;
mod report;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use bytes::Bytes;
use clap::{Parser, ValueEnum};
use hyper::Method;
use report::Report;

#[derive(Parser, Debug)]
#[command(about = "Generate HTTP load against the running workshop app")]
struct Cli {
    /// Server to benchmark (plain HTTP only)
    #[arg(long, default_value = "http://127.0.0.1:3000", value_parser = parse_target)]
    target: SocketAddr,

    /// Numbered workshop exercise to run
    #[arg(long, value_enum, default_value = "00")]
    exercise: Exercise,

    /// Request one path instead of using a workshop exercise
    #[arg(long, conflicts_with = "exercise")]
    path: Option<String>,

    /// Requests per second
    #[arg(long)]
    rate: Option<u32>,

    /// Total run time in seconds, including warmup
    #[arg(long, default_value_t = 15)]
    duration: u64,

    /// Don't compare against the previous lap
    #[arg(long)]
    reset: bool,

    /// Record a quiet baseline, the selected CPU incident, and recovery
    #[arg(long, conflicts_with_all = ["path", "rate", "duration", "reset"])]
    staged: bool,
}

fn main() {
    let cli = Cli::parse();
    if cli.staged {
        run_staged(cli.target, cli.exercise);
        println!(
            "Stop workshop-app with Ctrl-C; it will seal the trace and print its viewer link."
        );
        return;
    }

    let (name, requests) = match cli.path {
        Some(path) => ("custom", vec![Request::get(path)]),
        None => cli.exercise.requests(),
    };
    let config = Config {
        rate: cli.rate.unwrap_or_else(|| cli.exercise.default_rate()),
        duration: Duration::from_secs(cli.duration),
        reset: cli.reset,
        requests,
        results_dir: default_results_dir().join(name),
        ..Default::default()
    };

    run(cli.target, &config);

    println!("Stop workshop-app with Ctrl-C; it will seal the trace and print its viewer link.");
}

fn run_staged(addr: SocketAddr, exercise: Exercise) {
    let (history, incident_rate, incident_requests) = match exercise {
        Exercise::E05 => (
            "05-staged",
            150,
            vec![Request::get("/search?q=caf%C3%A9&suggestions=false")],
        ),
        _ => {
            eprintln!("hotlap: --staged is available for exercise 05");
            std::process::exit(2);
        }
    };

    println!("hotlap: staged recording (baseline -> incident -> recovery)");
    let stages = [
        ("baseline", 50, 12, vec![Request::get("/products/1")]),
        ("incident", incident_rate, 30, incident_requests),
        ("recovery", 50, 12, vec![Request::get("/products/1")]),
    ];

    for (name, rate, duration_secs, requests) in stages {
        println!("\n{name}: {duration_secs}s at {rate} req/s");
        let config = Config {
            rate,
            duration: Duration::from_secs(duration_secs),
            reset: true,
            requests,
            results_dir: default_results_dir().join(history).join(name),
            ..Default::default()
        };
        run(addr, &config);
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Exercise {
    // Baseline product traffic.
    #[value(name = "00")]
    E00,
    // Product traffic with rare catalog refreshes.
    #[value(name = "01")]
    E01,
    // Search traffic with a slow tail.
    #[value(name = "02")]
    E02,
    // Concurrent checkout traffic.
    #[value(name = "03")]
    E03,
    // Product traffic with optional recommendations.
    #[value(name = "04")]
    E04,
    // Search traffic for the first CPU investigation.
    #[value(name = "05")]
    E05,
    // Baseline product traffic used by the prepared noisy-neighbor trace.
    #[value(name = "06")]
    E06,
    // Storefront event ingestion with verbose application logging.
    #[value(name = "07")]
    E07,
}

impl Exercise {
    fn requests(self) -> (&'static str, Vec<Request>) {
        match self {
            Self::E00 => ("00", vec![Request::get("/products/1")]),
            Self::E01 => ("01", vec![Request::get("/products/1")]),
            Self::E02 => (
                "02",
                vec![
                    Request::get("/search?q=book").weight(19),
                    Request::get("/search?q=bo&suggestions=true"),
                ],
            ),
            Self::E03 => (
                "03",
                vec![
                    Request::json("/checkout", r#"{"product_id":5,"quantity":1}"#),
                    Request::json("/checkout", r#"{"product_id":6,"quantity":1}"#),
                    Request::json("/checkout", r#"{"product_id":7,"quantity":1}"#),
                    Request::json("/checkout", r#"{"product_id":8,"quantity":1}"#),
                ],
            ),
            Self::E04 => ("04", vec![Request::get("/products/1?recommendations=true")]),
            Self::E05 => (
                "05",
                vec![Request::get("/search?q=caf%C3%A9&suggestions=false")],
            ),
            Self::E06 => ("06", vec![Request::get("/products/1")]),
            Self::E07 => ("07", vec![storefront_event()]),
        }
    }

    fn default_rate(self) -> u32 {
        match self {
            Self::E03 => 25,
            Self::E05 => 150,
            Self::E07 => 5_000,
            _ => 500,
        }
    }
}

fn parse_target(value: &str) -> Result<SocketAddr, String> {
    let address = value
        .strip_prefix("http://")
        .unwrap_or(value)
        .trim_end_matches('/');
    address
        .parse()
        .map_err(|err| format!("expected http://HOST:PORT or HOST:PORT: {err}"))
}

struct Config {
    rate: u32,
    duration: Duration,
    /// Requests due during warmup are left out of the report.
    warmup: Duration,
    /// Number of keep-alive connections to the server.
    connections: usize,
    requests: Vec<Request>,
    results_dir: PathBuf,
    reset: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rate: 500,
            duration: Duration::from_secs(15),
            warmup: Duration::from_secs(2),
            connections: 50,
            requests: Vec::new(),
            results_dir: default_results_dir(),
            reset: false,
        }
    }
}

struct Request {
    method: Method,
    path: String,
    body: Bytes,
    weight: u32,
}

impl Request {
    fn get(path: impl Into<String>) -> Self {
        Self::new(Method::GET, path, Bytes::new())
    }

    fn new(method: Method, path: impl Into<String>, body: impl Into<Bytes>) -> Self {
        Self {
            method,
            path: path.into(),
            body: body.into(),
            weight: 1,
        }
    }

    fn json(path: impl Into<String>, body: impl Into<Bytes>) -> Self {
        Self::new(Method::POST, path, body)
    }

    fn weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }
}

fn storefront_event() -> Request {
    let context = "campaign=autumn-sale;referrer=/search?q=book;locale=en-US;".repeat(16);
    Request::json(
        "/events",
        format!(
            r#"{{"name":"product_viewed","session_id":"01K7D9WORKSHOP","product_id":1,"context":"{context}"}}"#
        ),
    )
}

fn run(addr: SocketAddr, config: &Config) {
    assert!(config.rate > 0, "hotlap: rate must be above 0");
    assert!(
        config.duration > config.warmup,
        "hotlap: duration must be longer than the warmup"
    );
    assert!(
        config.requests.iter().any(|request| request.weight > 0),
        "hotlap: no requests to send"
    );

    let outcome = load::generate(addr, config);
    let report = Report::new(config, &outcome);
    let last = if config.reset {
        None
    } else {
        Report::load(&config.results_dir)
    };
    report.print(last.as_ref(), &outcome);
    report.save(&config.results_dir);
}

fn default_results_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(|target| target.join("hotlap")))
        .unwrap_or_else(|| PathBuf::from("target/hotlap"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_accepts_url_or_socket_address() {
        let expected = "127.0.0.1:3000".parse().unwrap();
        assert_eq!(parse_target("http://127.0.0.1:3000").unwrap(), expected);
        assert_eq!(parse_target("127.0.0.1:3000").unwrap(), expected);
    }

    #[test]
    fn staged_conflicts_with_manual_workload_shape() {
        assert!(
            Cli::try_parse_from(["hotlap", "--exercise", "05", "--staged", "--rate", "100"])
                .is_err()
        );
    }

    #[test]
    fn exercises_use_numbered_cli_values() {
        let cli = Cli::try_parse_from(["hotlap", "--exercise", "03"]).unwrap();
        assert!(matches!(cli.exercise, Exercise::E03));
        assert!(Cli::try_parse_from(["hotlap", "--exercise", "mutex-contention"]).is_err());
    }

    #[test]
    fn custom_path_can_replace_the_default_exercise() {
        assert!(Cli::try_parse_from(["hotlap", "--path", "/health"]).is_ok());
    }
}
