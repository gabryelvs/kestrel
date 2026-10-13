# Kestrel

A free, honest FPS optimiser for VALORANT on Windows. Kestrel detects your hardware, finds what's
holding your frames back, and (in later versions) applies reversible tweaks picked for your PC.

> Kestrel is not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc.

## Status

Stage 2 (v0.2.0): the benchmark. A warm-up and 3 runs on a fixed route in VALORANT's Range measure average FPS,
1% and 0.1% lows, stutters and PC latency, and compare every later result with your starting point. Kestrel
changes nothing on your PC; it only saves benchmark results in its own folder.

Stage 1 (v0.1.0): hardware detection, Health Check and BIOS guide.

## Install a test build

1. Open the latest pre-release on the [Releases page](https://github.com/gabryelvs/kestrel/releases).
2. Download `Kestrel_<version>_x64-setup.exe` and run it. Installing over an older version is fine.
3. Test builds aren't signed yet, so Windows may show "Windows protected your PC". Click **More info → Run anyway**.

## What Kestrel will never do

- Read or change VALORANT's memory, or inject anything into it
- Load its own kernel driver
- Touch Riot Vanguard
- Tell you to turn off TPM or Secure Boot
- Permanently disable Defender, UAC or Windows Update
- Download anything while it runs (PresentMon, which measures frames, ships inside the installer)

## Development

Plans live in `docs/superpowers/plans/`. Windows builds run on GitHub Actions.

## Licence

GPL-3.0. See [LICENSE](LICENSE).
