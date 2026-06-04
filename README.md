# wxupd — rime-wanxiang updater

A cross-platform CLI to keep [rime-wanxiang](https://github.com/amzxyz/rime_wanxiang) scheme files, the LTS gram model, and supporting dictionaries up to date.

## Install

Download a binary from the [latest release](../../releases/latest), or build from source:

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

A personal access token (PAT) lifts the anonymous rate limit:

    export GITHUB_TOKEN=ghp_...
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
