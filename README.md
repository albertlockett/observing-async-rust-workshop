# Workshop - Beyond p99: Observing Async Rust 

This is the hands-on part of the workshop. 
We will use a small app that simulates storefront, put it under load via a separate program call hotlap, 
then inspect the app via Dial9 traces. 

## Setup

You need a recent Rust toolchain and the Dial9 CLI:

```bash
cargo install --locked dial9 --features cli
cd observing-async-rust-workshop
cargo build --release
```

Then keep three terminals open from this directory.

### 1. Viewer

Start this once and leave it running:

```bash
dial9 serve \
  --agg-source-dir traces \
  --agg-output-dir target/dial9-aggregate \
  --port 3001 \
  --local
```

You can then visit the viewer at http://localhost:3001, however for most exercises
the workshop app will give you direct links to the relevant traces.

### 2. Workshop app

```bash
cargo workshop-app
```

The app records until you stop it (e.g. Ctrl-C). On shutdown it seals the trace
and prints a link that opens that run directly in the viewer.

### 3. Hotlap

Hotlap is a small program that acts as a client of the workshop app, sending multiple
requests at once, and measuring the response times.
Each exercise gives you the exact command to run. It will look like this:

```bash
cargo hotlap --exercise 01
```

The first run records that exercise's baseline. After the fix, run the same
command again and Hotlap will compare it with the previous lap. Each exercise
keeps its own history.

If you run laps with different settings (e.g. `--duration` in seconds, `--rate`) 
add `--reset` to discard the previous laps so the comparison won't mislead you:

```bash
cargo hotlap --exercise 01 --reset
```

## The loop

For each exercise:

1. Start the app.
2. Run the exercise's Hotlap command.
3. Stop the app and open the trace link it prints.
4. Use the trace to form a hypothesis before reading hints.
5. Read one hint at the time.
6. Change the code with your ideas, or apply the prepared solution.
7. Restart the app and run the same workload again.

Restarting the app is important: it both picks up the code change and gives the
before/after runs separate traces.

Some exercises and solution patches are cumulative, so work through them in order.

## Exercises

Start at [exercise 00](exercises/00.md), then continue in order:

- [exercise 01](exercises/01.md)
- [exercise 02](exercises/02.md)
- [exercise 03](exercises/03.md)
- [exercise 04](exercises/04.md)
- [exercise 05](exercises/05.md)
- [exercise 06](exercises/06.md)
- [exercise 07](exercises/07.md)

Exercises 05–07 use prepared Linux profiles. You do not need Linux or
`perf`: the viewer command above serves the traces already in `traces/linux`.
Their exercise pages link directly to the relevant prepared run.

## Hints and solutions

Hints are folded into each exercise. Open them one at a time if you get stuck.

Every exercise also has a short explanation in
[`solutions/`](solutions/README.md):

```bash
git apply solutions/03.patch
```

The patches are meant as a quick solution to prove that the original diagnosis is correct;
some of the solutions are intentionally naive and will be disussed live.
