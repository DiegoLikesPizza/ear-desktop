# ear (desktop) [unofficial]

A native Windows control panel for Nothing and CMF audio devices, written in Rust.
It is a port of [ear-web](https://github.com/radiance-project/ear-web): same device
protocol and the same model/firmware capability table, but there's no browser and no
Web Serial. The app opens RFCOMM sockets straight to the earbuds through Winsock (`AF_BTH`).

## Why it's fast

- **No runtime, no webview.** It's a single 7.4 MB `.exe` that draws with egui on OpenGL.
  A warm start shows the window in ~40 ms, and the app uses 0 % CPU while idle (it only
  repaints on input or when the device sends something). Memory is ~140 MB on an NVIDIA
  GPU, most of which is the driver's OpenGL context.
- **Direct RFCOMM.** It connects by service UUID and Windows does the SDP lookup. There
  are no virtual COM ports and no drivers.
- **Remembers your device.** The Fast Pair model id is cached per Bluetooth address, and
  the last device is reconnected on launch. On a warm start, opening the window goes
  straight to "connected" in about 2 seconds.
- **Adaptive request pacing.** Each request goes out as soon as the previous one is
  answered (or after a short timeout). ear-web uses a fixed 100 ms gap, so the initial
  sync is usually several times quicker. Writes are acked one at a time, and a newer write
  of the same setting replaces a queued one, so dragging an EQ slider never floods the link.
- **Small assets.** Product renders are pre-shrunk from ~90 MB to ~3 MB, embedded in the
  binary and decoded lazily (only the 3 images for your model).

## Features

| Area | |
|---|---|
| Battery | Left / right / case (or single body), with charging state, live updates |
| Noise control | ANC / Transparency / Off and strength (High / Mid / Low / Adaptive), per model capability bits |
| Equalizer | Every preset lineup from ear-web (Balanced … / Dirac Opteo, Pop, Rock …), 3-band Custom EQ, 8-band Advanced EQ |
| Sound | Bass enhance (+ level), Spatial audio modes, Personal sound (Mimi / Audiodo) |
| Controls | Gesture remapping built from the per-firmware config: left / right / case / headphone buttons, noise-control cycle selection |
| Device | Find my earbuds, ear-tip fit test, in-ear detection, low lag mode, personalised ANC, long battery mode, Super Mic + mic mode, call transparency, walkie-talkie, anti-leakage |
| Connectivity | Audio codec (AAC / LHDC / LDAC, with reboot + auto-reconnect), dual connection list and connect/disconnect |

Supported devices: everything ear-web supports, including Nothing ear (1), ear (stick),
ear (2), Ear, Ear (a), Ear (open), Ear (3), Ear (3a), Headphone (1), Headphone (1) Pro,
Headphone (a), and CMF Buds / Buds Pro / Buds Pro 2 / Buds 2 / 2a / 2 Plus / Neckband Pro /
Headphone Pro / Clip Pro.

Not ported (yet): the Ear (1) case LED colour editor, the Precision-driver (third driver) EQ
tab, and the smart knob / dial pages.

## Use

1. Pair the device in **Settings → Bluetooth & devices** as usual.
2. Run `ear-desktop.exe` and pick the device. After that, it reconnects automatically.

## Build

Requires Windows, Rust ≥ 1.95 (the toolchain is pinned in `rust-toolchain.toml`), and the
MSVC linker (Visual Studio Build Tools, "Desktop development with C++").

```powershell
cargo build --release
```

The binary is written to `target\release\ear-desktop.exe`.

```powershell
cargo test
```

`tools/prepare_assets.py` regenerates `assets/` from an ear-web checkout (it needs Pillow):

```powershell
python tools/prepare_assets.py path\to\ear-web
```

## Layout

```
src/
  bt.rs        Winsock AF_BTH RFCOMM + paired device enumeration, Fast Pair model probe
  protocol.rs  frame encode / CRC-16 / reassembly / float codec, command ids
  device.rs    session worker: request queue with ack gating, frame decoding -> DeviceState
  config.rs    typed view of the per-model / per-firmware capability table
  models.rs    SKU table, preset lineups, per-model quirks, gesture labels
  app.rs       UI (device picker, Sound / Controls / Device tabs)
  theme.rs     visuals, fonts, small widgets
```

## Legal

AGPL-3.0, the same license as ear-web (see `LICENSE`). This app is not affiliated with, sponsored by, or endorsed by
Nothing Technology Limited. Product images, the Ndot font and the device configuration
data come from the ear-web project and remain the property of their owners.
