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

Log in with the Xiaohongshu app QR (native HTTP, no Python/browser dependency):

```sh
xhs login --qr --qr-file ./xhs-login.svg
# QR is also the default for bare `xhs login`; default timeout is 240 seconds.
```

The terminal displays the actual platform QR; optional SVG output is a new 0600 file. A confirmed scan is finalized and checked against the confirmed account ID before replacing the session. Expiry, risk challenges, guest sessions, and timeout preserve the previous login. No cookie values are printed.

Import a cookie JSON file or a Cookie header via stdin (without putting secrets in shell history):

```sh
cat /private/path/cookies.json | ./target/release/xhs login --cookie -
```

Session file defaults to `~/.xiaohongshu-cli/cookies.json` (compatible with jackwener/xiaohongshu-cli), and can be overridden using `--cookies PATH`. Keep this file private. Never include cookie files in source control. The CLI does not print cookies or request signing headers.

`edit` fetches the creator editing representation, changes only explicitly supplied fields, and uses the existing note ID with the platform's update endpoint. An omitted `--visibility` preserves the original visibility. Repeated `--image PATH` replaces card images; omitted images preserve the original files. `--desc ''` explicitly clears a description. The tool refuses incomplete editing representations rather than assuming that a private note is public or discarding existing media.

Compatibility aliases include `post`, `status`, `user-posts USER_ID`, `--body`, `--images`, `--private`, and `--xsec-token`; global `--json` is accepted. `topics KEYWORD` searches topics; publishing with repeated `--topic NAME_OR_ID` requires an exact match.

`read` accepts a signed note URL or `--xsec-token TOKEN`. Bare IDs do not reliably resolve arbitrary notes; pass the token returned by search or account lists. The third-party `user` / `user-notes` web endpoints may return HTTP 406 / code -1 even with valid cookies; this was also reproduced with upstream Python. `me` and `my-notes` are the verified account paths.

### Public video resources

`read` and `media` also resolve official HTTPS `xhslink.cn` / `xhslink.com` share links. `media` returns the selected video URL and metadata without downloading; add `--output` to download a complete MP4 with sound:

```sh
xhs read 'https://xhslink.cn/o/SHARE_ID'
xhs media 'https://xhslink.cn/o/SHARE_ID'
xhs media 'https://www.xiaohongshu.com/explore/NOTE_ID?xsec_token=TOKEN' --output /absolute/path/tutorial.mp4
```

The selector prefers H.264/EF4 MP4 variants with an audio track, then the largest resolution and bitrate. Fragmented or silent-only videos fail explicitly instead of producing a file missing its sound. Metadata includes chapter timestamps in milliseconds when available. Media URLs expire; run `media` again to obtain fresh URLs.

Share redirects and CDN requests use a separate client without session cookies. Redirects are restricted to official share/note hosts and media downloads to `xhscdn.com`; legacy CDN HTTP links are upgraded to HTTPS. Downloads reject HTTP errors, non-MP4 responses and incomplete transfers. The output directory must already exist. A completed temporary file is published atomically; existing output files are never overwritten, and failed transfers remove their temporary files.

Output is JSON. Platform errors, expired login sessions, and verification challenges produce a nonzero exit status. Mutations are sent once; ambiguous network failures are not retried automatically, to avoid duplicate posts. A successful request is not a guarantee that moderation has finished.

See `NOTICE`, `LICENSE` (Apache-2.0) and `LICENSE-xhshow` (MIT) for upstream attribution.

Images are inspected as a complete batch before uploads. Publish and image replacement include the actual width and height so portrait cards retain their intended aspect ratio.

### Document attachments

Creator web currently accepts one PDF, DOC, DOCX, PPT or PPTX document, up to 100 MiB. Game installers and archives are not supported documents. Upload separately so the resulting metadata can be reused without uploading again:

```sh
xhs upload-document /absolute/path/guide.pdf --json > attachment.json
xhs publish --title 'Title' --body 'Body' --image /absolute/path/cover.png --private --attachment attachment.json --json
xhs edit NOTE_ID --attachment attachment.json --json
```

`--attachment` takes the JSON receipt, **not** the document itself. On edit, omitted attachment metadata preserves the existing attachment; supplied metadata replaces only its attachment relation and keeps unrelated business binds. Upload and publish are separate operations: an uploaded document is not a published note. Requests are not automatically retried. Check the note's creator detail and business binds after publication; task eligibility, views and downloads are separate from upload success. Creator web contract checked 2026-10-25.

### Video upload and publication

The native video path accepts non-rotated H.264 MP4 files up to 1 GiB and four hours, with an explicit JPEG/PNG cover (native limit: 32 MiB). MP4 metadata is inspected in Rust; uploading does not require FFmpeg, a browser, or a JavaScript signing runtime. HEVC and fragmented files with no duration must be converted/remuxed first. Metadata inspection is not a full decoder or audiovisual quality check.

Upload material once and keep its receipt. Neither `upload-video` nor `video-status` creates a note or a draft:

```sh
xhs upload-video /absolute/path/final.mp4 --cover /absolute/path/cover.png --receipt material.json
xhs video-status --video-info material.json
xhs activities
xhs activities --topic-only
```

The upload receipt records video and cover file IDs and upload aliases, the video ID, dimensions, duration and the video's local MD5 fingerprint. It also retains the cover's SHA-256, MD5 and byte count for readback when the platform rewrites file IDs. It never records temporary upload credentials. It is created exclusively and preserves intermediate IDs if a later step fails. An existing receipt is never overwritten or automatically re-uploaded. Status queries may initially lack `has_first_frame`; query again after processing. Only a receipt missing its video ID uses the official file-key-to-video-ID fallback, which can allocate another media ID; retain the original upload receipt whenever possible.

Put the body in a UTF-8 file to retain real line breaks. Topic display markers such as `#用AI讲知识[话题]#` belong in that body; `--topic` resolves and sends the separate topic entity. Exact topic names are case-sensitive. Preview the complete payload before submitting:

```sh
xhs publish --title 'Title' --desc-file body.txt --video-info material.json \
  --topic 'AI替我说' --topic '用AI讲知识' --activity 45151 --visibility private --dry-run
```

`--activity` is optional. It reads the current official creator campaign record, checks that it is active, adds its topic entities, and includes the separate `ACTIVITY_COMPONENT` business relation. A campaign relation and its topic entities do not establish moderation, contest acceptance or reward eligibility. `--dry-run` resolves topics/campaigns and prints JSON; it never uploads material or creates a note. Body text containing a literal `\n` is rejected on the video path to catch accidental shell escaping.

After reviewing the video, cover, title, body, topics and visibility, publish with a **new** receipt path:

```sh
xhs publish --title 'Title' --desc-file body.txt --video-info material.json \
  --topic 'AI替我说' --topic '用AI讲知识' --activity 45151 --visibility public \
  --declaration ai-synthesis --receipt publication.json
xhs check-publish --receipt publication.json
```

Publication requires processing to report `has_first_frame=true`. The journal is created exclusively and synced before the POST; subsequent updates use same-directory temporary files and atomic replacement, all HTTP retries are disabled, and an existing publication receipt blocks another send. Success is reported as `accepted_unverified` until `check-publish` confirms the creator detail's media, title, body, topic IDs and visibility. It also checks the requested cover, campaign and native content declaration. The optional `--declaration ai-synthesis` selects “笔记含AI合成内容”.

Readback accepts the exact body or the platform's observed collapse of consecutive LF blank lines. The latter is explicitly reported with `description_exact=false`, a format warning and submitted/actual newline counts; other text changes fail. If file IDs become canonical IDs, verification requires the original material receipt, matching video MD5/video ID/dimensions/duration, and a complete original-cover download matching SHA-256/MD5/bytes/dimensions. The original submitted payload is preserved. Missing evidence fails verification.

If the request fails or its response is lost, the journal stays `unknown`; do not change the receipt path and resend. `check-publish` lists first-page title candidates when no note ID is available. Inspect all relevant `my-notes` pages and then use `check-publish --receipt publication.json --note NOTE_ID`. No candidates is not proof that publication failed. The command never republishes or rewrites the journal. Creator readback is separate from a successful public-feed read or campaign eligibility.

For a one-step upload and publication, `publish --video final.mp4 --cover cover.png ... --receipt publication.json` retains material metadata in `publication.json.video.json`. Reusing `upload-video` receipts is preferred when reviewing or publishing multiple attempts.

Protocol and live-validation limits are recorded in [docs/video-publishing.md](docs/video-publishing.md) and [VALIDATION.md](VALIDATION.md). Three explicitly approved public video notes were published once each and verified through creator readback and authenticated public-feed reads. Campaign task credit and rewards remain unverified.
