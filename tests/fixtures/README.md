# Test fixtures

`video-feed.json` is the existing redacted public-note fixture for resource selection.

`tiny-h264-aac.mp4` is a generated 0.4-second black 32×48 H.264 video with a silent mono AAC track (2,344 bytes). It contains no user media or account data. Regenerate it with:

```sh
ffmpeg -f lavfi -i color=c=black:s=32x48:r=10:d=0.4 -f lavfi -i anullsrc=r=8000:cl=mono \
  -t 0.4 -c:v libx264 -pix_fmt yuv420p -c:a aac -movflags +faststart tiny-h264-aac.mp4
```

The native parser tests do not execute FFmpeg. They inspect the committed bytes and reject truncated or non-MP4 inputs.
