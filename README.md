# Kestrel

A free, honest FPS optimiser for VALORANT on Windows. Kestrel detects your hardware, finds what's
holding your frames back, and (in later versions) applies reversible tweaks picked for your PC.

> Kestrel is not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc.

## Status

Stage 1 (v0.1.0): hardware detection, Health Check and BIOS guide. It only reads your PC; it changes nothing.

## Install a test build

1. Open the latest pre-release on the [Releases page](https://github.com/gabryelvs/kestrel/releases).
2. Download `Kestrel_0.1.0_x64-setup.exe` and run it.
3. Test builds aren't signed yet, so Windows may show "Windows protected your PC". Click **More info → Run anyway**.

## What Kestrel will never do

- Read or change VALORANT's memory, or inject anything into it
- Load its own kernel driver
- Touch Riot Vanguard
- Tell you to turn off TPM or Secure Boot
- Permanently disable Defender, UAC or Windows Update

## Development

Plans live in `docs/superpowers/plans/`. Windows builds run on GitHub Actions.

## Licence

GPL-3.0. See [LICENSE](LICENSE).
