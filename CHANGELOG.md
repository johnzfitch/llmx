# Changelog

## 2.2.1

### Fixed

- **BackendClient connection pooling**: Added `pool_idle_timeout(10s)` and `pool_max_idle_per_host(2)` to evict stale connections after backend restarts
- **Request timeout**: All HTTP proxy calls wrapped in 30s `tokio::time::timeout` -- dead backends can no longer hang the stdio proxy indefinitely
- **Retry with backoff**: Up to 2 retries with 100ms/200ms backoff on transient connection errors (refused, reset, broken pipe). Non-transient errors fail immediately
- **Token refresh on 401**: BackendClient re-reads auth token from disk when backend returns 401, recovering automatically from backend restarts that generate new tokens
- **Mutex starvation in refresh_impacted_indexes**: Split lock scope into three phases (read metadata, release during heavy I/O, save results) to prevent blocking HTTP handlers during index refresh
