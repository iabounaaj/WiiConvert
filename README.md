# WiiConvert (RVZ2WBFS)

Converts Wii disc images to WBFS format, automatically named and organised for USB loaders (USB Loader GX, WiiFlow, etc.).

Previously only di

## Supported input formats

| Format | Description |
|--------|-------------|
| `.rvz` | Dolphin compressed format |
| `.wia` | Wii Disc Archive |
| `.iso` / `.gcm` | Raw disc image |
| `.wbfs` | WBFS (re-conversion / rename) |
| `.ciso` | Compact ISO |

## Output structure

USB loaders expect games organised like this:

```
wbfs/
  Wii Party [SUPE01]/
    SUPE01.wbfs
  Mario Kart Wii [RMCE01]/
    RMCE01.wbfs
```

RVZ2WBFS creates this structure automatically. The game title and ID are read from the disc image, with correct display names sourced from the bundled WiiTDB database.

## Usage

1. Click **Browse…** next to Input and select your disc image
2. Click **Browse…** next to Output and select your USB drive's `wbfs` folder (or any folder)
3. Click **Convert to WBFS**

The output folder will open automatically when done.

## Building from source

Requires [Rust](https://rustup.rs/) 1.93 or later.

```sh
git clone https://github.com/YOUR_USERNAME/rvz2wbfs
cd rvz2wbfs
cargo build --release
```

The binary will be at `target/release/rvz2wbfs.exe`.

## Credits

- [nod](https://github.com/encounter/nod) — Wii/GameCube disc format library
- [WiiTDB](https://www.gametdb.com) — Game title database
