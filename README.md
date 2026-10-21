# xhs-cli

Native Rust command-line client for Xiaohongshu. The binary is `xhs`. It reads a local cookie session and implements HTTP requests, media upload, and request signing in Rust. Python is not a runtime dependency.

```sh
cargo build --release --locked -j 2
./target/release/xhs --help
./target/release/xhs me
./target/release/xhs search 'Rust'
./target/release/xhs read 'https://www.xiaohongshu.com/explore/NOTE_ID?xsec_token=TOKEN'
./target/release/xhs my-notes
./target/release/xhs publish --title 'Title' --desc 'Body' --image ./photo.png --visibility private
./target/release/xhs edit NOTE_ID --title 'Revised title'
```

Import a cookie JSON file or a Cookie header via stdin (without putting secrets in shell history):

```sh
cat /private/path/cookies.json | ./target/release/xhs login --cookie -
```

Session file defaults to `~/.xiaohongshu-cli/cookies.json` (compatible with jackwener/xiaohongshu-cli), and can be overridden using `--cookies PATH`. Keep this file private. Never include cookie files in source control. The CLI does not print cookies or request signing headers.

`edit` fetches the creator editing representation, changes only explicitly supplied fields, and uses the existing note ID with the platform's update endpoint. An omitted `--visibility` preserves the original visibility. Repeated `--image PATH` replaces card images; omitted images preserve the original files. `--desc ''` explicitly clears a description. The tool refuses incomplete editing representations rather than assuming that a private note is public or discarding existing media.

Compatibility aliases include `post`, `status`, `user-posts USER_ID`, `--body`, `--images`, `--private`, and `--xsec-token`; global `--json` is accepted. `topics KEYWORD` searches topics; publishing with repeated `--topic NAME_OR_ID` requires an exact match.

`read` accepts a signed note URL or `--xsec-token TOKEN`. Bare IDs do not reliably resolve arbitrary notes; pass the token returned by search or account lists. The third-party `user` / `user-notes` web endpoints may return HTTP 406 / code -1 even with valid cookies; this was also reproduced with upstream Python. `me` and `my-notes` are the verified account paths.

Output is JSON. Platform errors, expired login sessions, and verification challenges produce a nonzero exit status. Mutations are sent once; ambiguous network failures are not retried automatically, to avoid duplicate posts. A successful request is not a guarantee that moderation has finished.

See `NOTICE`, `LICENSE` (Apache-2.0) and `LICENSE-xhshow` (MIT) for upstream attribution.
