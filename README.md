# FETH Bench EXP

Helps benched characters keep up in Fire Emblem: Three Houses without boosting
the units you actually deploy.

[![build](https://github.com/jinghaihan/feth-bench-exp/actions/workflows/build.yml/badge.svg)](https://github.com/jinghaihan/feth-bench-exp/actions/workflows/build.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> [!WARNING]
> Version 0.1.0 is an in-game test build. The stage-clear timing, deployment
> flags, and save persistence have not yet been verified in Eden or on hardware.
> Back up your save before enabling it, and test a disposable battle first.

## Requirements

- Fire Emblem: Three Houses **1.2.0**, Build ID
  `89048449BA238C8CF565518B83BF02D3`
- The FE3H-specific Skyline loader, such as
  [Aldebaran](https://github.com/three-houses-research-team/aldebaran-rs)

## Install

Download the NRO or installable ZIP from
[Releases](https://github.com/jinghaihan/feth-bench-exp/releases). With the
FE3H Skyline loader already installed, copy the NRO to:

```text
sdmc:/atmosphere/contents/010055D009F78000/romfs/skyline/plugins/feth-bench-exp.nro
```

On Eden, merge the ZIP into the emulator's **emulated SD card**, not its
ordinary game-mod folder. On Windows, this is typically `%AppData%\eden\sdmc`.
The ZIP contains this plugin only; it does not include the Skyline loader.
Fully restart Eden after installation. To disable it, remove the NRO and
restart the emulator. Do not replace other plugins when merging directories.

## Behavior

At the battle stage-clear screen, the plugin calculates the integer average
level of formally deployed, currently usable player units and subtracts **5**.
It then brings eligible undeployed recruits below that floor up to it. Adjutants
count as benched units: they receive their normal battle EXP first, then only
the catch-up needed to reach the floor. Units already at or above the floor
are unchanged. Dead, unavailable, and unrecruited units are excluded.

EXP is granted one level at a time through the game's 1.2.0 EXP/level-up
function. The plugin does not directly write a unit's level or change skill
EXP, class mastery, support points, or weapon ranks. It makes no save metadata
of its own, but any earned EXP and stat gains can be saved normally and are not
undone by removing the plugin. It is designed to coexist with
[`feth-fixed-growths`](https://github.com/jinghaihan/feth-fixed-growths): the
game's level-up function calls the fixed-growth hook if both are active.
This interaction still needs an in-game test.

The gap is the `LEVEL_GAP` constant in `src/plan.rs`. Change it from `5` to
`3` or `7` and rebuild the NRO to select a different floor.

## First test

1. Back up a save, then record one deployed unit's level and one low-level
   bench unit's level, EXP, and stats.
2. Win one short battle without deploying the bench unit. Include an adjutant
   if possible.
3. After the stage-clear screen and a manual save, check whether the bench
   unit rose toward `floor(deployed average) - 5`; deployed units should get
   only their original EXP.
4. Reload that save and confirm that the levels and stats persist. If the game
   crashes, remove only `feth-bench-exp.nro`, keep the backed-up save, and
   provide Eden's log and the point of failure.

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
