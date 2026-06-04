# rime-wanxiang-updater (`wxupd`) — Design Spec

- **Date**: 2026-06-04
- **Status**: Approved for planning
- **Author**: wangkezun + Claude

## 1. Purpose

A single-binary, cross-platform CLI that keeps a user's local [rime-wanxiang](https://github.com/amzxyz/rime_wanxiang) installation up to date, including:

1. **Scheme** files (the wanxiang configuration zip for the user's chosen variant: full pinyin / 自然码 / 小鹤 / 微软 / 极点汉字 / …)
2. **Gram** model (the LTS n-gram language model, e.g. `wanxiang-lts-zh-hans.gram`)
3. **Extra resources** such as 中英混合词库

The CLI also redeploys Rime (default on, opt-out) so updates take effect immediately.

## 2. Goals & non-goals

**Goals**

- Cross-platform: macOS (Squirrel), Windows (Weasel), Linux (iBus/Fcitx5 Rime)
- Resources are versioned independently — `scheme`, `gram`, `dict` each have their own upstream tag and update cadence
- Safe defaults: never clobber a user's `*.custom.yaml`, user dictionaries, or `installation.yaml`
- Resilient to flaky `github.com` access via configurable mirror prefixes (e.g. `ghfast.top`, `ghproxy.com`)
- Atomic per-resource install with automatic backup → `rollback` is always available

**Non-goals**

- Editing Rime config on the user's behalf (no `wanxiang.custom.yaml` munging)
- GUI / TUI — terminal only
- Authoring new Rime schemes — pure consumer of upstream releases
- Mirroring the resources ourselves — we only fetch from upstream + user-configured mirrors
- Resumable downloads (YAGNI; assets are tens of MB at most)

## 3. CLI surface

```
wxupd check [--json]
wxupd update [--no-deploy] [--force] [scheme|gram|dict ...]
wxupd rollback [scheme|gram|dict ...]
wxupd config show
wxupd config set <key>=<value>
```

Exit codes (scriptable):

| Code | Meaning |
|---|---|
| 0 | Success / everything up-to-date |
| 1 | Generic error (parse, IO, etc.) |
| 2 | Network / download failure |
| 3 | Install failure (post-download) |
| 10 | `check` found an available update |
| 130 | User interrupt (Ctrl-C) |

`GITHUB_TOKEN` env var, when set, is sent as `Authorization: Bearer …` to lift the anonymous 60 req/h rate limit. Useful in CI (PAT via secret) and for heavy local users.

## 4. Architecture

### 4.1 Module layout

```
crates/wxupd/
├── src/
│   ├── main.rs            # clap entry, error → exit code mapping
│   ├── cli.rs             # clap derive structs for subcommands
│   ├── config.rs          # load/save config.toml (toml_edit preserves comments)
│   ├── manifest.rs        # load/save manifest.json, history pruning
│   ├── github.rs          # release lookup + mirror fallback
│   ├── http.rs            # download with progress + sha256 streaming
│   ├── platform.rs        # Rime user-dir detection + deploy command
│   ├── safe_list.rs       # glob-based protect list
│   ├── backup.rs          # tar.zst write/extract
│   ├── resource/
│   │   ├── mod.rs         # trait Resource + dispatch
│   │   ├── scheme.rs
│   │   ├── gram.rs
│   │   └── dict.rs
│   └── ops/
│       ├── check.rs
│       ├── update.rs
│       └── rollback.rs
└── tests/
    ├── integration_update.rs   # wiremock + assert_cmd
    └── fixtures/
```

### 4.2 Dependencies (initial)

| Crate | Purpose |
|---|---|
| `clap` (derive) | Arg parsing |
| `tokio` (rt-multi-thread, fs, signal) | Async runtime |
| `reqwest` (rustls-tls, json, stream) | HTTP |
| `serde`, `serde_json` | JSON |
| `toml`, `toml_edit` | Config read + comment-preserving write |
| `zip` | Unpack scheme assets |
| `tar`, `zstd` | Backup format |
| `sha2` | Streaming hash |
| `regex`, `globset` | Asset matching + SafeList |
| `semver` | Version compare (with lexicographic fallback) |
| `directories` | XDG / Windows / macOS dirs |
| `indicatif` | Progress bars |
| `anyhow`, `thiserror` | Errors |
| `tracing`, `tracing-subscriber` | Logging |
| `chrono` | Timestamps in manifest |

Dev: `wiremock`, `assert_cmd`, `tempfile`, `predicates`, `insta` (snapshot for `check --json`).

### 4.3 Resource abstraction

```rust
pub trait Resource: Send + Sync {
    fn id(&self) -> &'static str;        // "scheme" | "gram" | "dict"
    fn repo(&self) -> &str;              // "amzxyz/rime_wanxiang"
    fn asset_pattern(&self, cfg: &Config) -> Regex;
    async fn latest_remote(&self, gh: &Github) -> Result<RemoteRef>;
    async fn install(
        &self,
        downloaded: &Path,
        rime_dir: &Path,
        safe: &SafeList,
    ) -> Result<InstallReport>;
}

pub struct RemoteRef {
    pub tag: String,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_size: u64,
    pub sha256: Option<String>,
    pub published_at: DateTime<Utc>,
}

pub struct InstallReport {
    pub files_written: Vec<PathBuf>,   // relative to rime_dir
    pub files_skipped: Vec<PathBuf>,   // hit SafeList
}
```

Per-resource specifics:

| Resource | Repo (verified at impl time) | Asset selector | Install action |
|---|---|---|---|
| `scheme` | `amzxyz/rime_wanxiang` | `wanxiang-{variant}-.*\.zip` | unzip → SafeList filter → copy to `rime_dir/` |
| `gram` | `amzxyz/RIME-LMDG` *(verify)* | `wanxiang-lts-zh-hans\.gram` | copy single file to `rime_dir/` |
| `dict` | TBD at impl (likely same wanxiang repo, separate asset) | `cn_en_.*\.dict\.yaml` | copy yaml(s) to `rime_dir/`, SafeList applies |

The trait stops at "fetched bytes → installed bytes". Download / sha verification / backup / manifest writes live in the outer `ops::update` orchestrator so each `Resource` impl stays small.

## 5. State files

### 5.1 `config.toml`

Location:
- Linux/macOS: `$XDG_CONFIG_HOME/wxupd/config.toml` (fallback `~/.config/wxupd/config.toml`)
- Windows: `%APPDATA%\wxupd\config.toml`

```toml
[scheme]
variant = "pinyin"          # set on first run (interactive)

[paths]
rime_user_dir = ""          # empty = auto-detect; explicit = override

[network]
mirrors = [
  "https://ghfast.top",
  "https://ghproxy.com",
]
timeout_secs = 60

[deploy]
auto = true

[safe_list]
extra = []
```

### 5.2 `manifest.json`

Location:
- Linux/macOS: `$XDG_DATA_HOME/wxupd/manifest.json` (fallback `~/.local/share/wxupd/manifest.json`)
- Windows: `%LOCALAPPDATA%\wxupd\manifest.json`

```json
{
  "schema_version": 1,
  "resources": {
    "scheme": {
      "tag": "v8.3",
      "asset_name": "wanxiang-pinyin-v8.3.zip",
      "sha256": "abc123…",
      "installed_at": "2026-06-04T12:00:00Z",
      "files_installed": ["wanxiang.schema.yaml", "lua/…"],
      "history": [
        {
          "tag": "v8.2",
          "backup": "backups/scheme/v8.2.tar.zst",
          "installed_at": "2026-05-12T08:00:00Z",
          "files_installed": ["wanxiang.schema.yaml", "..."]
        }
      ]
    }
  }
}
```

`history` keeps the last 3 backups per resource by default; older `tar.zst` files are pruned after a successful install.

Backups live in `$XDG_DATA_HOME/wxupd/backups/<id>/<tag>.tar.zst` (or the Windows equivalent).

### 5.3 Default SafeList

```
*.custom.yaml
installation.yaml
user.yaml
*.userdb*
*.userdb.txt
sync/**
build/**
```

Matched with `globset`, relative to `rime_user_dir`. Users append via `config.toml → [safe_list].extra`.

## 6. Operation flows

### 6.1 `check`

1. Load `config` + `manifest` (missing manifest → all resources reported as "not installed").
2. Concurrently for each resource:
   - `GET /repos/{repo}/releases/latest` through mirror chain; first 200 wins, otherwise propagate last error.
   - Pick the asset whose name matches `asset_pattern(cfg)`.
   - Compare `RemoteRef.tag` against `manifest.resources[id].tag`.
3. Render a table (`resource | local | remote | status`).
4. `--json` emits a structured object (snapshot-tested with `insta`).
5. Exit code: 0 if all current, 10 if any has an update, 1 on errors.

### 6.2 `update [resources...]`

1. Same as `check` to obtain `RemoteRef`s.
2. Keep only resources with `status = has-update`, unless `--force`.
3. Empty → print "all up-to-date", exit 0.
4. Concurrently download to `$XDG_CACHE_HOME/wxupd/staging/<id>/<asset_name>`:
   - stream → progress bar + `sha2::Sha256` in the same pass.
   - verify against `RemoteRef.sha256` if upstream publishes one.
5. **Sequential install phase** (no concurrent writes into `rime_user_dir`):
   - For each resource:
     1. Compute the set of files the install *would* write (peek into zip / inspect asset).
     2. Diff against SafeList; record skipped files for the report.
     3. Backup current `manifest.files_installed` entries that the install would overwrite, into `backups/<id>/<old_tag>.tar.zst`.
     4. Run `resource.install()`.
     5. Update `manifest` (new `tag`, `files_installed`); push old version into `history`.
     6. Prune `history` beyond the keep-N limit (default 3).
6. If `deploy.auto && !--no-deploy`: run platform deploy command; failure is a warning, not a rollback (deploy is idempotent and user-retryable).
7. Print summary table per resource: `old → new`, written count, skipped count.

**Per-resource atomicity**: if step 5.4 fails, immediately restore that resource from the backup taken in 5.3. Other resources that already succeeded stay applied — we never half-roll-back a globally-mixed state.

### 6.3 `rollback [resources...]`

1. Default: rollback every resource that has `history`; resources without history are reported and skipped (not an error). When the user names specific resources, missing `history` IS an error and exits non-zero.
2. For each target resource:
   - Read `manifest.resources[id].history[0]` (most recent prior version, including its `files_installed`).
   - Compute `to_delete = current.files_installed − history[0].files_installed`. Delete those files so the rolled-back state doesn't leak files the new install added.
   - Extract `backups/<id>/<old_tag>.tar.zst` over `rime_dir/` to restore the prior contents of files that *did* exist in both versions.
   - Swap: current `(tag, files_installed)` is pushed onto `history`; the prior entry becomes current. `rollback` itself is therefore reversible (`rollback` after `rollback` = redo).
3. Run platform deploy unless `--no-deploy`.

`rollback` is purely local; no network access.

## 7. Platform handling

`platform::rime_user_dir()`:

| OS | Detection order |
|---|---|
| macOS | `config.paths.rime_user_dir` → `$HOME/Library/Rime` |
| Windows | `config.paths.rime_user_dir` → `%APPDATA%\Rime` |
| Linux | `config.paths.rime_user_dir` → `$IBUS_RIME_USER_DATA_DIR` → `$HOME/.config/ibus/rime` → `$XDG_DATA_HOME/fcitx5/rime` → `$HOME/.local/share/fcitx5/rime` |

If multiple Linux candidates exist, prefer the one that already contains files. If none exist on Linux, fail with an explicit "set `paths.rime_user_dir`" message.

`platform::deploy()`:

| OS | Command |
|---|---|
| macOS | `/Library/Input Methods/Squirrel.app/Contents/MacOS/Squirrel --reload` (or `osascript` fallback) |
| Windows | `WeaselDeployer.exe /deploy` (resolved via registry / `%ProgramFiles%`) |
| Linux | `rime_deployer --build "$rime_user_dir" "$rime_shared_dir"` if available; otherwise warn |

Deploy failures are reported as warnings but don't change the exit code unless every other step also failed.

## 8. Error handling

- Top-level `main` returns `anyhow::Result<()>`; the err chain is logged via `tracing` to stderr.
- Boundary typed errors (`thiserror`) where the outer code needs to branch:
  - `GithubError::RateLimited` → hint to set `GITHUB_TOKEN` or add a mirror.
  - `GithubError::NotFound` → asset pattern likely outdated, point to `--force` and config.
  - `ChecksumError` → retry once via the next mirror, then give up.
  - `InstallError` → triggers per-resource backup restore.
- Staging dir is **not** cleaned on failure — it's a forensic trail. Next run detects and reports leftover staging, offering `--prune-cache`.
- Output style: error line, then a `tip:` line with a concrete next action.

## 9. Testing

| Layer | Tools | Coverage |
|---|---|---|
| Unit | `cargo test` | SafeList globbing, version compare, manifest (de)serialization, `tar.zst` round-trip, mirror URL composition |
| GitHub integration | `wiremock` | Fake release JSON + fake asset bytes; mirror fallback, rate-limit, 404, asset-pattern mismatch |
| End-to-end | `assert_cmd` + `tempfile` | Drive `check`/`update`/`rollback` against a temp `rime_user_dir`; assert files land, SafeList skips, manifest mutates, exit codes |
| Snapshot | `insta` | `check --json` output stability |
| Platform | GitHub Actions matrix (macOS/Windows/Linux) | All of the above; deploy command is exercised only as a smoke test (binary discoverable) |

**Out of scope for automated tests**

- Hitting real `github.com` from CI (rate-limit + flake).
- Verifying Rime actually loads the new dictionary (Rime's own concern).

## 10. CI / Release

GitHub Actions:

- `test.yml`: on push / PR — matrix (macOS, Windows, Linux) → `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
- `release.yml`: on `v*` tag → build three-platform binaries (`x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, `aarch64-apple-darwin`), attach to GitHub Release. Likely tool: `cargo-dist`.

A repo secret `GITHUB_PAT` (PAT) can be exposed as `GITHUB_TOKEN` for jobs that need to hit `api.github.com` (the integration tests use `wiremock` so this is only for any future real-API smoke test). The default `${{ secrets.GITHUB_TOKEN }}` is sufficient for the release job's `gh release upload`.

## 11. Future work (explicitly deferred)

- Resumable downloads
- Auto-update of `wxupd` itself
- Pre-update dry-run / diff display
- Detecting and warning when `rime-wanxiang` upstream introduces a breaking layout change
- TUI / GUI front-end

## 12. Open implementation-time questions

- Confirm the exact repo + asset names for `gram` and `dict` resources (the current spec lists best-guess values). Implementation should start by reading the actual `amzxyz/rime_wanxiang` (and any sibling) releases page and codifying the asset regex.
- Confirm Squirrel's deploy CLI invocation on current macOS — there are several stale recipes in the wild.
- Decide the exact `variant` enum (which double-pinyin schemes ship today).
