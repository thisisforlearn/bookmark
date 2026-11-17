# Troubleshooting

Run `bookmark doctor` first — it checks the data directory, database, FTS5, models, port,
disk space, and SIMD features, and prints a FAIL line for anything wrong.

## The server won't start

| Message | Cause | Fix |
|---|---|---|
| `cannot bind 127.0.0.1:8080` | port in use | `--port 8081`, or stop the other process |
| `cannot open database` | wrong path / permissions | check `data_dir` in the config and its `0700` perms |
| `migration failed` | old/corrupt schema | restore from a `bookmark backup` archive |

## Search shows "degraded"

One or more models are missing or invalid. Either ignore it (lexical search still works) or:

```sh
bookmark model train all --mode quick
bookmark model verify
```

then restart the server.

## Typo correction is weak

- The vocabulary is per user and built from your bookmarks. With very few bookmarks it leans on
  the built-in seed list. Save more bookmarks and retrain:
  `bookmark model train spell --mode full`.
- Very short words (≤ 2 chars) are never "corrected" — there isn't enough signal.

## Crawling fails

| `error_kind` | Meaning |
|---|---|
| `blocked` | SSRF guard: the URL resolves to a private/loopback address |
| `robots` | robots.txt disallows it |
| `status` | 4xx/5xx from the site |
| `too_large` | body exceeded 2 MB |
| `not_html` | it's a PDF/image; only the URL is stored |
| `timeout` | the site took > 15 s |

`bookmark crawl --all` retries failures with exponential backoff; non-retryable errors are
marked `failed`.

## Termux kills the process

See [TERMUX.md](TERMUX.md): wake-lock, `bookmark watch`, Termux:Boot, and battery exemption.

## Login problems

- Forgot password: create a new admin with
  `BOOKMARK_USER_PASSWORD=… bookmark user add admin2 --admin`.
- Sessions expire after 30 days; signing in again rotates the token.
- Cookies require the same host you opened the UI on (loopback ≠ LAN IP).

## Uploading screenshots fails

- Max 5 MB; the file must decode as PNG/JPEG/WebP.
- The form must include the CSRF field (the built-in upload control does).

## Recovering from a bad state

```sh
bookmark backup                    # always take one before surgery
mv ~/.local/share/bookmark/bookmark.db{,.bak}
# extract a known-good archive into the data directory
```
