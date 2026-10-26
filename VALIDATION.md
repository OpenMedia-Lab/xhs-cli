# Validation (2026-10-21)

Native debug binary was used with the existing signed-in account. The installed Python `xhs` command was not replaced.

- Account info (`me`), creator list (`my-notes`), note search (22 results for Rust), signed-token note read, creator editing detail, topic search: passed.
- Published a private image note, ID `6ad83ab20000000025005140`.
- Edited the title in place: same ID, changed title, unchanged description, same image, private visibility retained. All five assertions passed using creator detail readback.
- Edited the body and replaced its image in place: same ID, changed body, different uploaded image ID, omitted title preserved, private visibility retained. All five assertions passed using creator detail readback.
- Deleted this test note. Follow-up detail returned platform code `-9106`, `该笔记已被删除`. No test note remains.
- Third-party profile/user-note endpoints returned HTTP 406 / platform code -1. The profile failure also reproduces with upstream Python, so these commands remain available but are not claimed as live-verified successes.

`cargo test --locked`: 16 passed (8 CLI integration + 8 contract tests), no failures.

The test suite covers creator AES signing and xhshow hash vectors, omitted-field/private/media preservation, flattened creator-detail normalization, image replacement, exact topic resolution, CLI aliases and private stdin session import, atomic replacement preserving prior auth on a failed write, and JSON runtime errors. Tests were observed failing before implementation.

On this Mac, direct `rustc` linking against the selected MacOSX27 SDK failed with an unknown `arm64e.x1` architecture in the SDK stub. Verification used `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk` and `CARGO_BUILD_JOBS=2` without changing user configuration. GitHub CI builds and tests on Linux, including a release build.

## 2026-10-25 document attachment validation

- Added `upload-document` and `bindings`, plus `publish/edit --attachment` metadata receipts.
- Creator web contract: one PDF/DOC/DOCX/PPT/PPTX, <=100 MiB, `web_doc` scene; local validation checks extension, basic file signature, nonempty payload and size before upload.
- `cargo test`: 23 tests passed (including document binding preservation, unsupported installer/signature/size rejection, attachment-only edit parsing).
- Real authorized private momo note: uploaded a 3-page Chinese README PDF, published 8 images with 5 topics, verified creator detail privacy type 1 and ATTACHMENT business relation.
- Attachment-only edit on the same private note verified the replacement document ID and metadata, unchanged gallery file IDs, title, body and topic IDs, and preserved privacy type 1.
- Publication receipts remain outside the repository under the local postdeck momo output directory. No session credentials are included here. Activity eligibility and download/read thresholds were not claimed or tested.

## 2026-10-25 public video validation (isolated snapshot)

- Added official share-link resolution to `read`, plus `media NOTE [--token TOKEN] [--output FILE]`.
- Unit/integration coverage includes a real public video feed fixture, H.264/audio selection across multiple variants, non-video and silent-fragment rejection, external redirect/CDN rejection, redirect loops, HTTPS normalization, non-overwriting output and cleanup on truncated/non-video/interrupted transfers.
- `cargo test`: 32 passed, zero failures; `cargo build`: passed. No dependencies or global configuration changed.
- Read-only live validation: `xhs media https://xhslink.cn/o/3NjfcrXedKu --output /private/tmp/video-skill-research-20261025/tutorial.mp4` resolved the share URL and downloaded note `6adb6000000000003301beee`.
- `ffprobe` verified a 380.819667-second, 29,504,410-byte MP4: 720×960 H.264 video plus 48 kHz stereo AAC audio. No posts or account state were modified, and no global binary was installed.

## 2026-10-26 native video upload and publication preflight

- Preserved the pre-existing document, image, share-link and media-download changes. The baseline had 33 passing tests. Their source/test files and fixture were compared byte-for-byte with the saved baseline after video implementation.
- Added native bounded MP4 metadata inspection, streaming video PUT, explicit cover upload, reusable private material receipts, processing queries, official topic/campaign resolution, UTF-8 body files, dry-run video payloads, publication journals and creator readback verification.
- Current official creator bundles confirmed the video permit, ROS upload channels, header video ID, `videoid` fallback, `query_transcode`, cover schema, deep snake-case publication payload and `ACTIVITY_COMPONENT` relation. Source references and the delivered flow are in `docs/video-publishing.md`.
- Live uploaded only two synthetic materials: a 2,344-byte, 0.4-second H.264/AAC fixture through the main ROS channel, and a 3,205,042-byte, three-second H.264/AAC clip through `ros-upload-d4.xhscdn.com`. Both had separately uploaded explicit PNG covers; later status queries returned `has_first_frame=true`.
- The fallback video-ID handshake was tested using the first uploaded file. It allocated another media ID, so the implementation preserves the original header ID and only uses fallback when no video ID exists. No note or draft was created by these material operations.
- Live topic search resolved exactly `AI替我说` (`6acca064000000002302cc07`) and `用AI讲知识` (`6aca516f000000002200ce88`). Live campaign lookup found `45151`; dry-run JSON preserved both entities, its independent activity business relation, private visibility and three actual body newlines.
- Added regression coverage for explicit platform error codes/conflicting success fields, verification HTTP statuses, bounded MP4 inspection and truncation, upload host restrictions, topic/visibility/cover/video metadata, activity bindings, exact readback, actual-newline dry-run execution blocking an existing unknown publication journal before authentication/network access, failed atomic updates preserving the submitting payload, 3xx upload rejection, bounded JPEG/PNG cover validation, and malformed processing responses returning errors instead of panicking.
- At this preflight checkpoint, `cargo fmt --all --check`, `cargo test --locked --all-targets` (49 passed), `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and `cargo build --release --locked` passed. Installed that release binary at `~/.local/bin/xhs`, preserving the previous binary outside the repository. Installed-binary help and a real topic/campaign dry-run were verified; installed and build SHA-256 hashes matched.
- No note or draft had been created at that checkpoint. Later authorized end-to-end publication is recorded below. Uploads near the size/duration ceiling and interrupted large transfers remain untested.

## 2026-10-26 approved public video publication

- The user explicitly approved three complete portrait films, independent covers, exact titles/bodies/topics, public visibility, activity 45151, native AI synthesis declaration and submission order. Each video note was submitted once, with an exclusive durable journal retaining the original payload. No recovery repost or metadata edit was performed.
- All three 1080×1920 H.264/AAC originals (186.267, 193.834 and 257 seconds) were accepted. Creator detail retained the matching titles, public privacy type 0, both exact topic IDs, activity relation and `userDeclarationBind.origin=2`. Creator lists returned visible entries without permission errors, and authenticated public-feed reads returned the three matching video notes.
- The platform replaced upload-permit file IDs with canonical IDs. Independent comparison verified original video MD5, video ID, dimensions and rounded duration for all three. Original cover downloads were byte-for-byte equal to approved PNG covers, including SHA-256, MD5, byte count and dimensions.
- Material evidence was added to verification copies of the older local receipts while preserving their originals and exact submitted payloads. New journals retain this material directly. Installed `check-publish` returned `verified=true` for all three, with explicit alias/canonical identity attestations; it did not mutate journals or submit notes.
- The platform collapsed consecutive LF blank lines: nine submitted LF characters became five in each body. All other characters matched. CLI reports `description_exact=false` and `platform_collapsed_blank_lines` with a format warning and counts; it does not claim exact paragraph spacing. Regression tests reject other text/whitespace changes and insufficient or mismatching media evidence.
- Final local checks passed: 61 tests (`cargo test --locked --all-targets`), `cargo fmt --all --check`, strict Clippy, `git diff --check`, and a locked release build. Installed/build SHA-256: `370553c6ad6fd8832f937c03f6db880d9981c44d0959d8c691351656ee970709`.
- Activity association and public-feed availability are verified. Effective campaign task count, moderation outcomes beyond the observed readable state, reward eligibility and vouchers remain unverified. The available general task API and campaign landing page did not expose a reliable campaign-specific 3/3 counter. Private publication records and credentials are not committed.

## 2026-10-26 original-ID video edit preparation

- Later user feedback and live creator-list readback identified a recommendation/search restriction on one of those notes. The notice explicitly cited the outbound blog URL in its body. Creator detail still showed enabled/public; the earlier readable state had not established moderation approval. The other two currently lacked punishment fields but contained the same kind of body link.
- Verified the current official editor's retained `noteId`, upload-triggered `isReplaceVideo`, serialized `video_info.replaced_video` and PUT note/update endpoint. Added reusable replacement material, UTF-8 body files, read-only previews and a private original-ID edit journal with a single PUT and no automatic retry.
- Twelve new regressions cover media/cover replacement, body-only preservation, full privacy/native topics/activity/declaration/unknown fields, incomplete material rejection, old CLI compatibility, pure previews, existing-journal rejection before authentication, ambiguous outcomes and three-way original/target/payload ID consistency. An explicitly different response ID remains unknown and cannot redirect reconciliation.
- Final local checks passed 73 tests, formatting, strict Clippy, locked release build and diff checks. Installed release SHA-256: `95b79ca3bd618fcc3f5c58a8a9892dd1880dc9d6a190b913f4be5fdf23cbce41`. An independent review reran all 12 edit tests. All three cached real creator details passed offline replacement and body-only construction; no account data is included in public fixtures.
- Installed CLI performed real read-only previews for all three original IDs with new material: exact revised bodies, public0, both topics, activity45151 and declaration2 retained; each new material had a ready first frame. No update, deletion, appeal or new note was sent during that preparation checkpoint.

## 2026-10-26 approved original-ID replacement

- The user explicitly approved all three complete revised films, independent covers and full new copy, and required editing the old notes rather than reposting. Each was updated through exactly one journaled PUT on its original ID. The returned ID, journal's original/target/payload IDs and final creator readback ID all matched. No deletion, appeal, new-note POST or recovery retry was performed.
- All three installed `check-publish` calls returned `verified=true`, `operation=edit` and the original note ID. Original video MD5/video ID/dimensions/rounded duration and exact downloaded original-cover SHA-256/MD5/bytes/dimensions matched the newly approved material. The revised body, both native topics, activity45151, privacy0 and declaration2 survived. Consecutive LF blank lines again collapsed explicitly (8→4 per body); all other text matched.
- Authenticated public-feed reads returned all three matching original video-note IDs and revised descriptions. Creator-list entries for all three showed `tab_status=1`, `ref_tip=0`, permission_code0 and no permission message. The previously restricted note no longer returned punishment/advice fields, unlike its captured before-state3/2. These are observations after editing, not a promise of future recommendation volume or activity credit.
- The same installed release and reviewed source passed 73 tests, strict Clippy, formatting and the locked release build. Private before/after responses, original publication journals and edit journals remain outside the repository. No campaign-specific effective3/3 counter or reward verification became available.
