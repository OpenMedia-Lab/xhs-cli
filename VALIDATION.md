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
