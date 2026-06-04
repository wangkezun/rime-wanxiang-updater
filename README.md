# wxupd — rime-wanxiang updater

A cross-platform CLI to keep [rime-wanxiang](https://github.com/amzxyz/rime_wanxiang) scheme files, the LTS gram model, and supporting dictionaries up to date.

## Install

Download a binary from the [latest release](../../releases/latest), or build from source:

    cargo install --path .

## Quick start

    wxupd config set scheme.variant=pinyin   # or flypy, zrm, mspy, ...
    wxupd check
    wxupd update
    wxupd rollback           # if something looks off

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
