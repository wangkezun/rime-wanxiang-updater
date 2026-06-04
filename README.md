# wxupd — rime-wanxiang updater

A cross-platform CLI to keep [rime-wanxiang](https://github.com/amzxyz/rime_wanxiang) scheme files, the LTS gram model, and supporting dictionaries up to date.

## Install

### Homebrew (macOS / Linux)

    brew tap wangkezun/rime-wanxiang-updater https://github.com/wangkezun/rime-wanxiang-updater
    brew install wangkezun/rime-wanxiang-updater/wxupd

Upgrades come the normal way:

    brew update && brew upgrade wxupd

### Binary download

Grab the right artifact for your platform from the [latest release](../../releases/latest), `chmod +x`, and put it on `PATH`.

### Build from source

    cargo install --path .

## Quick start

    wxupd config set scheme.variant=base   # see Variants below for other options
    wxupd check
    wxupd update
    wxupd rollback           # if something looks off

## Variants

`scheme.variant` accepts these upstream-defined names (default `base`):

| Variant | Description |
|---|---|
| `base` | 全拼基础方案 |
| `flypy-fuzhu` | 小鹤双拼辅助码 |
| `zrm-fuzhu` | 自然码辅助码 |
| `wubi-fuzhu` | 五笔辅助码 |
| `moqi-fuzhu` | 墨奇辅助码 |
| `hanxin-fuzhu` | 汉心辅助码 |
| `shouyou-fuzhu` | 手游辅助码 |
| `shyplus-fuzhu` | 山卡 Plus 辅助码 |
| `tiger-fuzhu` | 虎码辅助码 |
| `wx-fuzhu` | 五笔仿写辅助码 |

## Network

If `github.com` is slow or blocked, configure mirrors:

    wxupd config set network.mirrors="https://ghfast.top,https://ghproxy.com"

A personal access token (PAT) lifts the anonymous 60 req/h rate limit. Most users don't need one; if you do, **don't paste it into `config.toml`** — that file may end up in dotfile backups or sync targets. Set `GITHUB_TOKEN` from a secret store instead.

### Reusing `gh auth token` (recommended, all platforms)

If you already use the [GitHub CLI](https://cli.github.com/), reuse its token:

**macOS / Linux** — add to `~/.zshrc` (or `~/.bashrc`):
```bash
export GITHUB_TOKEN=$(gh auth token 2>/dev/null)
```

**Windows** — add to your PowerShell profile (`$PROFILE`):
```powershell
$env:GITHUB_TOKEN = (gh auth token 2>$null)
```

### Without `gh`

**macOS** — store in Keychain once:
```bash
security add-generic-password -a wxupd -s github-token -w "ghp_xxxx"
# then in ~/.zshrc:
export GITHUB_TOKEN=$(security find-generic-password -a wxupd -s github-token -w 2>/dev/null)
```

**Linux** — use the [`pass`](https://www.passwordstore.org/) password store or freedesktop secret-service:
```bash
pass insert wxupd/github-token
# then in ~/.bashrc:
export GITHUB_TOKEN=$(pass wxupd/github-token 2>/dev/null)
```

**Windows** — Credential Manager via the [`CredentialManager`](https://www.powershellgallery.com/packages/CredentialManager) PowerShell module:
```powershell
Install-Module CredentialManager -Scope CurrentUser
New-StoredCredential -Target wxupd-github -UserName wxupd -Password "ghp_xxxx" -Persist LocalMachine
# then in $PROFILE:
$cred = Get-StoredCredential -Target wxupd-github -ErrorAction SilentlyContinue
if ($cred) { $env:GITHUB_TOKEN = $cred.GetNetworkCredential().Password }
```

### Quick & dirty (not recommended for shared machines)

Persist a user-level env var directly. Plaintext in your user profile but at least out of dotfile sync:

```powershell
# Windows
[Environment]::SetEnvironmentVariable("GITHUB_TOKEN", "ghp_xxxx", "User")
```

After whichever method above, verify:

    wxupd check

## Subcommands

| Command | Description |
|---|---|
| `wxupd check [--json]` | Compare local manifest vs upstream releases |
| `wxupd update [--no-deploy] [--force] [scheme\|gram\|dict ...]` | Download + install + (optional) deploy |
| `wxupd rollback [scheme\|gram\|dict ...]` | Restore the previous installed version |
| `wxupd config show / set KEY=VALUE` | Inspect or modify `config.toml` |

## Exit codes

`0` success • `1` generic error • `2` network/download • `3` install • `10` `check` found updates • `130` interrupted.

## Safe list

Files matching the safe list are never overwritten by `update`. Defaults:

```
*.custom.yaml, installation.yaml, user.yaml,
*.userdb*, *.userdb.txt, sync/**, build/**
```

Extend via `[safe_list].extra` in `config.toml`.

## License

MIT
