use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::Config;
use crate::load::{Outcome, Status};

const FILE_NAME: &str = "last-lap.json";

/// Changes smaller than this are shown as `~`, since laptop runs are noisy.
const NOISE_PERCENT: f64 = 10.0;

/// Max is a single sample and swings a lot between identical runs.
const MAX_NOISE_PERCENT: f64 = 50.0;

/// A client lag above this gets a warning under the report.
const LAG_WARNING: Duration = Duration::from_millis(20);

/// Summary of one run, after dropping warmup.
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub rate: u32,
    pub duration_secs: u64,
    #[serde(default)]
    pub warmup_secs: u64,
    pub p50_ms: Option<f64>,
    pub p90_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// Successful requests per second.
    pub throughput: f64,
    pub errors: u64,
}

impl Report {
    pub(crate) fn new(config: &Config, outcome: &Outcome) -> Self {
        let measured = outcome.samples.iter().filter(|s| s.due >= config.warmup);
        // A request that never came back still took at least as long as it
        // waited, so it belongs in the latencies. A failed one has no latency
        // worth reporting.
        let mut latencies: Vec<Duration> = measured
            .clone()
            .filter(|s| s.status != Status::Failed)
            .map(|s| s.latency)
            .collect();
        latencies.sort_unstable();
        let errors = measured.filter(|s| s.status != Status::Ok).count() as u64;
        // Only count requests that finished inside the measured window, not
        // ones that trickled in after the schedule ended.
        let finished_in_window = outcome
            .samples
            .iter()
            .filter(|s| {
                s.status == Status::Ok
                    && (config.warmup..=config.duration).contains(&(s.due + s.latency))
            })
            .count();
        let measured_secs = config.duration.saturating_sub(config.warmup).as_secs_f64();

        Self {
            rate: config.rate,
            duration_secs: config.duration.as_secs(),
            warmup_secs: config.warmup.as_secs(),
            p50_ms: percentile(&latencies, 50.0),
            p90_ms: percentile(&latencies, 90.0),
            p99_ms: percentile(&latencies, 99.0),
            max_ms: latencies.last().map(millis),
            throughput: finished_in_window as f64 / measured_secs,
            errors,
        }
    }

    pub(crate) fn load(dir: &Path) -> Option<Self> {
        serde_json::from_slice(&fs::read(dir.join(FILE_NAME)).ok()?).ok()
    }

    pub(crate) fn save(&self, dir: &Path) {
        let json = serde_json::to_vec_pretty(self).expect("report serializes");
        let result = fs::create_dir_all(dir).and_then(|()| fs::write(dir.join(FILE_NAME), json));
        if let Err(err) = result {
            eprintln!("hotlap: couldn't save report in {}: {err}", dir.display());
        }
    }

    pub(crate) fn print(&self, last: Option<&Report>, outcome: &Outcome) {
        let style = Style::detect();

        println!();
        if cfg!(debug_assertions) {
            let warning = "warning: debug build, these numbers mean little. Build with --release.";
            println!("{}", style.paint(YELLOW, warning));
        }
        println!(
            "hotlap: {}s at {} req/s, first {}s dropped",
            self.duration_secs, self.rate, self.warmup_secs,
        );
        println!();
        println!(
            "{:<10}{:>10}{:>10}{:>10}{:>10}{:>9}{:>8}",
            "", "p50", "p90", "p99", "max", "req/s", "errors"
        );
        self.print_row("this lap");

        if let Some(last) = last {
            last.print_row("last lap");

            let latencies = [
                (self.p50_ms, last.p50_ms, NOISE_PERCENT),
                (self.p90_ms, last.p90_ms, NOISE_PERCENT),
                (self.p99_ms, last.p99_ms, NOISE_PERCENT),
                (self.max_ms, last.max_ms, MAX_NOISE_PERCENT),
            ];
            let mut row = format!("{:<10}", "change");
            for (now, before, noise) in latencies {
                row += &change(now, before, Better::Lower, noise, 10, style);
            }
            let (now, before) = (Some(self.throughput), Some(last.throughput));
            row += &change(now, before, Better::Higher, NOISE_PERCENT, 9, style);
            row += &error_change(self.errors, last.errors, 8, style);
            println!("{row}");

            if (last.rate, last.duration_secs) != (self.rate, self.duration_secs) {
                note(
                    style,
                    DIM,
                    &format!(
                        "last lap used {}s at {} req/s, so the comparison is off",
                        last.duration_secs, last.rate
                    ),
                );
            }
        }

        if outcome.unfinished > 0 {
            note(
                style,
                YELLOW,
                &format!(
                    "{} requests never came back. They count above as the time they had waited, so the real latencies are worse",
                    outcome.unfinished
                ),
            );
        }
        if outcome.max_lag > LAG_WARNING {
            note(
                style,
                YELLOW,
                &format!(
                    "the bench client itself fell up to {} behind schedule, so some latency may be the client's",
                    format_ms(millis(&outcome.max_lag))
                ),
            );
        }
        println!();
    }

    fn print_row(&self, label: &str) {
        let cell = |ms: Option<f64>| ms.map(format_ms).unwrap_or_else(|| "-".into());
        println!(
            "{:<10}{:>10}{:>10}{:>10}{:>10}{:>9.0}{:>8}",
            label,
            cell(self.p50_ms),
            cell(self.p90_ms),
            cell(self.p99_ms),
            cell(self.max_ms),
            self.throughput,
            self.errors,
        );
    }
}

/// A line under the table, set off by a blank line.
fn note(style: Style, code: &str, text: &str) {
    println!();
    println!("{}", style.paint(code, text));
}

/// Nearest-rank percentile.
fn percentile(sorted: &[Duration], p: f64) -> Option<f64> {
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted.get(rank.saturating_sub(1)).map(millis)
}

fn millis(d: &Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn format_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{ms:.2}ms")
    } else if ms < 1000.0 {
        format!("{ms:.1}ms")
    } else {
        format!("{:.2}s", ms / 1000.0)
    }
}

enum Better {
    Lower,
    Higher,
}

/// Percent for small changes, a multiplier past 2x so that big improvements
/// don't all round to -100%. Red if worse, green if better.
fn change(
    now: Option<f64>,
    last: Option<f64>,
    better: Better,
    noise_percent: f64,
    width: usize,
    style: Style,
) -> String {
    let (Some(now), Some(last)) = (now, last) else {
        return format!("{:>width$}", "-");
    };
    if now <= 0.0 || last <= 0.0 {
        return format!("{:>width$}", "-");
    }
    let percent = (now - last) / last * 100.0;
    if percent.abs() < noise_percent {
        return style.paint(DIM, &format!("{:>width$}", "~"));
    }

    let ratio = now / last;
    let text = if ratio >= 2.0 {
        format!("↑{}x", format_factor(ratio))
    } else if ratio <= 0.5 {
        format!("↓{}x", format_factor(1.0 / ratio))
    } else {
        format!("{percent:+.0}%")
    };
    let worse = match better {
        Better::Lower => ratio > 1.0,
        Better::Higher => ratio < 1.0,
    };
    style.paint(if worse { RED } else { GREEN }, &format!("{text:>width$}"))
}

fn format_factor(factor: f64) -> String {
    if factor < 10.0 {
        format!("{factor:.1}")
    } else {
        format!("{factor:.0}")
    }
}

fn error_change(now: u64, last: u64, width: usize, style: Style) -> String {
    let diff = now as i64 - last as i64;
    let text = if diff == 0 {
        "~".to_string()
    } else {
        format!("{diff:+}")
    };
    let text = format!("{text:>width$}");
    match diff {
        0 => style.paint(DIM, &text),
        d if d > 0 => style.paint(RED, &text),
        _ => style.paint(GREEN, &text),
    }
}

const RED: &str = "31";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const DIM: &str = "2";

#[derive(Clone, Copy)]
struct Style {
    color: bool,
}

impl Style {
    fn detect() -> Self {
        Self {
            color: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let sorted: Vec<Duration> = (1..=10).map(Duration::from_millis).collect();
        assert_eq!(percentile(&sorted, 50.0), Some(5.0));
        assert_eq!(percentile(&sorted, 90.0), Some(9.0));
        assert_eq!(percentile(&sorted, 99.0), Some(10.0));
        assert_eq!(percentile(&[], 50.0), None);
    }

    #[test]
    fn changes_read_as_noise_percent_or_multiple() {
        let plain = Style { color: false };
        let cell = |now: f64, last: f64, noise: f64| {
            change(Some(now), Some(last), Better::Lower, noise, 8, plain)
                .trim()
                .to_string()
        };
        assert_eq!(cell(10.0, 10.5, NOISE_PERCENT), "~");
        assert_eq!(cell(15.0, 10.0, NOISE_PERCENT), "+50%");
        assert_eq!(cell(6600.0, 14.0, NOISE_PERCENT), "↑471x");
        assert_eq!(cell(14.0, 6600.0, NOISE_PERCENT), "↓471x");
        // Max swings run to run, so it takes a bigger move to count as real.
        assert_eq!(cell(24.0, 17.0, MAX_NOISE_PERCENT), "~");
    }

    #[test]
    fn durations_switch_units() {
        assert_eq!(format_ms(0.42), "0.42ms");
        assert_eq!(format_ms(13.75), "13.8ms");
        assert_eq!(format_ms(6600.0), "6.60s");
    }
}
