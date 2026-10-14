use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::{IsTerminal, Write};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1::{self, SendRequest};
use hyper::header::{CONTENT_TYPE, HOST, HeaderValue};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};

use crate::{Config, Request};

/// How long to wait for in-flight requests once the schedule is done.
const GRACE: Duration = Duration::from_secs(2);

#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum Status {
    Ok,
    /// Refused, failed mid-flight, or answered with a non-2xx status.
    Failed,
    /// Still waiting when the run ended.
    Unfinished,
}

pub(crate) struct Sample {
    /// When the request was due, relative to the start of the run.
    pub due: Duration,
    /// Counted from `due` rather than from when the request went out, so
    /// time spent waiting behind a stalled server is included. For an
    /// unfinished request this is how long it had waited, which is a floor
    /// on what its latency would have been.
    pub latency: Duration,
    pub status: Status,
}

pub(crate) struct Outcome {
    pub samples: Vec<Sample>,
    /// Measured requests that were due but never finished.
    pub unfinished: u64,
    /// Worst delay between a request being due and the client queueing it.
    /// A large value means the client itself fell behind, not the server.
    pub max_lag: Duration,
}

struct Job {
    index: usize,
    due: Duration,
    request: usize,
}

#[derive(Default)]
struct Progress {
    completed: Cell<u64>,
    errors: Cell<u64>,
}

/// One benchmark run and the state its tasks share.
struct Lap<'a> {
    config: &'a Config,
    addr: SocketAddr,
    host: HeaderValue,
    /// Gap between scheduled requests.
    interval: Duration,
    /// Request indices to cycle through, in the configured proportions.
    mix: Vec<usize>,
    start: Instant,
    jobs: Mutex<mpsc::UnboundedReceiver<Job>>,
    samples: RefCell<Vec<Sample>>,
    /// Whether the request with this index is still outstanding.
    outstanding: RefCell<Vec<bool>>,
    max_lag: Cell<Duration>,
    progress: Progress,
}

pub(crate) fn generate(addr: SocketAddr, config: &Config) -> Outcome {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build hotlap runtime")
        .block_on(generate_async(addr, config))
}

async fn generate_async(addr: SocketAddr, config: &Config) -> Outcome {
    let (tx, rx) = mpsc::unbounded_channel::<Job>();
    let expected = config.rate as usize * config.duration.as_secs() as usize;
    let host = HeaderValue::from_str(&addr.to_string()).expect("socket address is a valid header");
    let lap = Lap {
        config,
        addr,
        host,
        interval: Duration::from_secs_f64(1.0 / f64::from(config.rate)),
        mix: mix(&config.requests),
        start: Instant::now(),
        jobs: Mutex::new(rx),
        samples: RefCell::new(Vec::with_capacity(expected)),
        outstanding: RefCell::new(Vec::with_capacity(expected)),
        max_lag: Cell::new(Duration::ZERO),
        progress: Progress::default(),
    };

    let workers = futures_util::future::join_all((0..config.connections).map(|_| lap.worker()));
    let deadline = lap.start + config.duration + GRACE;

    tokio::select! {
        _ = async { tokio::join!(lap.schedule(tx), workers) } => {}
        _ = tokio::time::sleep_until(deadline.into()) => {}
        _ = lap.show_progress() => {}
    }
    if std::io::stderr().is_terminal() {
        eprint!("\r\x1b[2K");
    }

    lap.finish()
}

impl Lap<'_> {
    /// Queues each request at its scheduled time, then closes the channel so
    /// the workers stop once they've drained it.
    async fn schedule(&self, tx: mpsc::UnboundedSender<Job>) {
        for index in 0.. {
            let due = self.interval.mul_f64(index as f64);
            if due >= self.config.duration {
                break;
            }
            tokio::time::sleep_until((self.start + due).into()).await;
            let lag = self.start.elapsed().saturating_sub(due);
            self.max_lag.set(self.max_lag.get().max(lag));

            self.outstanding.borrow_mut().push(true);
            let _ = tx.send(Job {
                index,
                due,
                request: self.mix[index % self.mix.len()],
            });
        }
    }

    /// Takes one queued request at a time over a single connection.
    async fn worker(&self) {
        let mut conn = None;
        loop {
            let Some(job) = self.jobs.lock().await.recv().await else {
                return;
            };
            let request = &self.config.requests[job.request];
            let status = match send(self.addr, &self.host, &mut conn, request).await {
                true => Status::Ok,
                false => Status::Failed,
            };

            self.samples.borrow_mut().push(Sample {
                due: job.due,
                latency: self.start.elapsed().saturating_sub(job.due),
                status,
            });
            self.outstanding.borrow_mut()[job.index] = false;
            self.progress
                .completed
                .set(self.progress.completed.get() + 1);
            if status == Status::Failed {
                self.progress.errors.set(self.progress.errors.get() + 1);
            }
        }
    }

    /// Adds the requests that never came back, so a stalled server shows up in
    /// the latencies instead of only in the error count.
    fn finish(self) -> Outcome {
        let elapsed = self.start.elapsed();
        let mut samples = self.samples.into_inner();
        let stuck: Vec<usize> = self
            .outstanding
            .into_inner()
            .into_iter()
            .enumerate()
            .filter_map(|(index, pending)| pending.then_some(index))
            .collect();

        samples.extend(stuck.iter().map(|&index| {
            let due = self.interval.mul_f64(index as f64);
            Sample {
                due,
                latency: elapsed.saturating_sub(due),
                status: Status::Unfinished,
            }
        }));

        let unfinished = stuck
            .iter()
            .filter(|&&index| self.interval.mul_f64(index as f64) >= self.config.warmup)
            .count() as u64;

        Outcome {
            samples,
            unfinished,
            max_lag: self.max_lag.get(),
        }
    }

    /// Redraws a one-line progress bar on stderr. Never returns.
    async fn show_progress(&self) {
        if !std::io::stderr().is_terminal() {
            return std::future::pending().await;
        }

        const WIDTH: usize = 20;
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        // (time, completed) over the last second, for a smoothed req/s.
        let mut window = VecDeque::new();
        loop {
            tick.tick().await;
            let now = Instant::now();
            let completed = self.progress.completed.get();
            window.push_back((now, completed));
            while window
                .front()
                .is_some_and(|&(t, _)| now - t > Duration::from_secs(1))
            {
                window.pop_front();
            }
            let (then, completed_then) = window[0];
            let secs = (now - then).as_secs_f64();
            let rate = if secs > 0.0 {
                (completed - completed_then) as f64 / secs
            } else {
                0.0
            };

            let elapsed = self.start.elapsed();
            let label = if elapsed < self.config.warmup {
                "warming up"
            } else if elapsed < self.config.duration {
                "on the lap"
            } else {
                "finishing"
            };
            let elapsed = elapsed.min(self.config.duration);
            let filled = (elapsed.as_secs_f64() / self.config.duration.as_secs_f64() * WIDTH as f64)
                as usize;

            let mut stderr = std::io::stderr().lock();
            let _ = write!(
                stderr,
                "\r\x1b[2K{label:<10}  ▕{}{}▏  {:>2}s / {}s  {:>6.0} req/s  {} errors",
                "█".repeat(filled),
                "░".repeat(WIDTH - filled),
                elapsed.as_secs(),
                self.config.duration.as_secs(),
                rate,
                self.progress.errors.get(),
            );
            let _ = stderr.flush();
        }
    }
}

/// Request indices repeated by weight, so cycling through them gives the
/// right mix without randomness.
fn mix(requests: &[Request]) -> Vec<usize> {
    requests
        .iter()
        .enumerate()
        .flat_map(|(i, r)| std::iter::repeat_n(i, r.weight as usize))
        .collect()
}

async fn send(
    addr: SocketAddr,
    host: &HeaderValue,
    conn: &mut Option<SendRequest<Full<Bytes>>>,
    request: &Request,
) -> bool {
    if conn.as_ref().is_none_or(SendRequest::is_closed) {
        *conn = connect(addr).await;
    }
    let Some(sender) = conn.as_mut() else {
        return false;
    };
    if sender.ready().await.is_err() {
        *conn = None;
        return false;
    }

    let mut builder = hyper::Request::builder()
        .method(request.method.clone())
        .uri(request.path.as_str())
        .header(HOST, host);
    if !request.body.is_empty() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    let req = builder
        .body(Full::new(request.body.clone()))
        .expect("invalid request");

    let ok = match sender.send_request(req).await {
        Ok(response) => {
            let status = response.status();
            response.into_body().collect().await.is_ok() && status.is_success()
        }
        Err(_) => false,
    };
    if !ok && sender.is_closed() {
        *conn = None;
    }
    ok
}

async fn connect(addr: SocketAddr) -> Option<SendRequest<Full<Bytes>>> {
    let stream = TcpStream::connect(addr).await.ok()?;
    // Nagle's algorithm would add tens of milliseconds to small requests.
    stream.set_nodelay(true).ok()?;
    let (sender, connection) = http1::handshake(TokioIo::new(stream)).await.ok()?;
    tokio::spawn(connection);
    Some(sender)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_repeats_requests_by_weight() {
        let requests = vec![Request::get("/a"), Request::get("/b").weight(3)];
        assert_eq!(mix(&requests), vec![0, 1, 1, 1]);
    }
}
