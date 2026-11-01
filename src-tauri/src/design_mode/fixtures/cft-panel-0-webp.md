# WebP capture fixture

The source `cft-panel-0.png` is the retained Chrome for Testing panel capture.
On 2026-11-01, convert it with the installed libwebp encoder:

```sh
cwebp -quiet -q 25 cft-panel-0.png -o cft-panel-0.webp
```

The WebP file tests Rust data URL acceptance and persistence. It is not a
recording of a Chrome extension command reply.
