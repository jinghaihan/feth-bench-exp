# FETH Bench EXP

Helps benched characters keep up in Fire Emblem: Three Houses without boosting
the units you actually deploy.

[![build](https://github.com/jinghaihan/feth-bench-exp/actions/workflows/build.yml/badge.svg)](https://github.com/jinghaihan/feth-bench-exp/actions/workflows/build.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Requirements

- Fire Emblem: Three Houses **1.2.0**, Build ID
  `89048449BA238C8CF565518B83BF02D3`
- The FE3H-specific Skyline loader, such as
  [Aldebaran](https://github.com/three-houses-research-team/aldebaran-rs)

## Install

Back up your saves before installing the plugin.

### Nintendo Switch (Atmosphere)

Download the NRO or installable ZIP from
[Releases](https://github.com/jinghaihan/feth-bench-exp/releases). With the
FE3H Skyline loader already installed, copy the NRO to:

```text
sdmc:/atmosphere/contents/010055D009F78000/romfs/skyline/plugins/feth-bench-exp.nro
```

Fully restart the game after installation. To disable the plugin, remove the
NRO and restart the game. The ZIP contains this plugin only, not the Skyline
loader. Do not replace other plugins when merging directories.

### Emulators (Eden)

On Eden, merge the ZIP into the emulator's **emulated SD card**, not its
ordinary game-mod folder. On Windows, this is typically `%AppData%\eden\sdmc`.
The ZIP contains this plugin only; it does not include the Skyline loader.
Fully restart Eden after installation. To disable it, remove the NRO and
restart the emulator. Do not replace other plugins when merging directories.

## Behavior

On the normal battle-result exit path, the plugin captures the IDs of actual
player-side fighters and adjutants from the battle actors before cleanup.
After the game writes those actors back and finishes cleanup, it calculates
the integer average of the settled levels of those fighters and subtracts
**3** by default. Save deployment flags cleared during cleanup do not change
the captured roster. Eligible bench units and adjutants below the floor are
then brought up to it; adjutants keep their normal battle EXP first. Units
already at or above the floor are unchanged. Dead, unavailable, and
unrecruited units are excluded. Actual fighters never receive catch-up EXP.

Each initialized battle can trigger catch-up only once. Retreat and the
alternative event/rewind exit paths do not grant catch-up. Missing or duplicate
fighter records, a changed save pointer, or an empty valid fighter roster
cause catch-up to be skipped rather than calculating from a partial roster.
The old, repeatedly invoked stage-clear UI activation hook is no longer used.

EXP is granted one level at a time through the game's 1.2.0 EXP/level-up
function. The plugin does not directly write a unit's level or change skill
EXP, class mastery, support points, or weapon ranks. It makes no save metadata
of its own, but any earned EXP and stat gains can be saved normally and are not
undone by removing the plugin. It is designed to coexist with
[`feth-fixed-growths`](https://github.com/jinghaihan/feth-fixed-growths): the
game's level-up function calls the fixed-growth hook if both are active.

## Configuration

Put `feth-bench-exp.cfg` in the root of the console's SD card (or the
emulator's virtual SD card), not alongside the NRO:

```ini
level_gap=3
diagnostic_log=true
log_max_kib=2048
```

All settings are optional. `level_gap` accepts 0–99; 0 catches bench units up
to the rounded-down deployed average. It works even with logging disabled.
`diagnostic_log` defaults to `false`. `log_max_kib` accepts 64–65536 KiB and
defaults to 2048 KiB (2 MiB). Existing files containing only `diagnostic_log`
remain supported. Missing or invalid configuration uses the defaults.

Fully restart the game (and the emulator, if used) after changing settings.

## Diagnostic log

When enabled, the plugin writes to `sdmc:/feth-bench-exp.log`. Once the size
limit is reached, it drops the oldest complete lines, keeps roughly the newest
half, and continues writing. An existing oversized log is trimmed the same way.
It does not stop logging just because the file is full.

The log includes:

- Game version, every executable signature check, and whether the hook was
  installed. A signature mismatch includes its offset, expected instruction,
  and actual instruction.
- Battle initialization, each battle-exit entry, its exit mode, and whether
  catch-up was armed. The log header uses `diagnostic_schema=2`.
- Captured battle actor IDs and normalized role flags, followed by the frozen
  fighter IDs, settled level sum and count, rounded-down average, gap, target
  floor, and number of recipients. Adjutants are distinguished from ordinary
  bench recipients.
- Every save roster slot's character ID, level, EXP, and raw flags after the
  game's writeback/cleanup, before and after catch-up.
- Each next-level threshold and EXP amount passed to the game's EXP function,
  its observed result, and any reason catch-up stopped.
- Reasons catch-up was skipped or stopped. These observations do not prove
  that a later manual save and reload persist the change.

For example, the default gap with a deployed average of 20 produces a target
of 17. A bench unit already at level 17 will not gain EXP. If the file contains
startup checks but no `battle_exit` entries after leaving the result screen, the selected hook
did not run; if there are entries, the plan and per-unit reasons show why
catch-up did or did not happen.

The logger uses a separate SD mount from the durability plugin. Logging may
slow the game; set `diagnostic_log=false` and fully restart when diagnostics
are no longer needed. A log-file I/O failure stops logging, not catch-up EXP.

Win one ordinary battle with logging enabled, check the bench units afterward,
then send `feth-bench-exp.log` and the emulator log if available. No action-by-action
notes are needed. If no file appears, the emulator log is needed to distinguish
a plugin load failure from an incorrect SD path or a file I/O error.

## Checking catch-up

1. Back up a save, then record one deployed unit's level and one low-level
   bench unit's level, EXP, and stats.
2. Win one short battle without deploying the bench unit. Include an adjutant
   if possible.
3. Leave the stage-clear/result screens, then manually save and check whether the bench
   unit rose toward `floor(deployed average) - level_gap`; deployed units
   should get only their original EXP.
4. Reload that save and confirm that the levels and stats persist. If the game
   crashes, remove only `feth-bench-exp.nro`, keep the backed-up save, and
   provide Eden's log and the point of failure.

The v0.1.3 timing change is checked against the 1.2.0 executable and covered
by host regression tests, including the cleared-flags and low-level-adjutant
cases. Hardware/emulator save-and-reload verification is still required.

## Credits

This is an independent implementation. These projects provided public
technical references or runtime tooling; their source and assets are not
included here.

- [Aldebaran](https://github.com/three-houses-research-team/aldebaran-rs) —
  FE3H Skyline loader and runtime research.
- [Progenitor](https://github.com/three-houses-research-team/Progenitor) —
  public FE3H save-structure reference.
- [skyline-rs](https://github.com/ultimate-research/skyline-rs) — plugin runtime.

Fire Emblem and related names are trademarks of Nintendo and Intelligent
Systems. This unofficial fan project is not affiliated with or endorsed by
them.

## License

[MIT](./LICENSE) License © [jinghaihan](https://github.com/jinghaihan)
