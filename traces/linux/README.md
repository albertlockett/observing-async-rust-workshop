# Curated Linux traces

These traces cover exercises that need Linux CPU profiling. Attendees inspect
them but do not need Docker or `perf` locally.

```bash
dial9 serve --agg-source-dir traces --agg-output-dir target/dial9-aggregate --port 3001 --local
```

Generated runs are written under `traces/local`; curated traces live under
`traces/linux`. The same viewer serves both. Curated traces use Dial9's
production `version=1` object layout, so they work in the density browser
locally and can later be uploaded to S3 unchanged. Their top-level prefixes are
only exercise numbers, so the browser URL does not reveal the diagnosis.
