# FE3H 1.2.0 battle writeback boundary

Profile: Build ID `89048449BA238C8CF565518B83BF02D3`. Addresses below are
relative to main text, verified against the user's local main NSO.

## Why the UI hook was removed

`0x14A260` activates stage-clear UI state. It does not perform battle actor
writeback and can be called repeatedly. Reading save deployment flags there
can produce a one-fighter average after cleanup. Raising a save record before
actor writeback can also be overwritten by the actor's older copy.

## Data and timing

- `0x1A1440` initializes/restores battle actors from save records. It arms one
  catch-up cycle; initialization alone never grants EXP.
- `0xA9C60` calls `0xC2D10`, which copies the first `0xC4` bytes of a save unit
  into a separate battle actor. Level, EXP, stats and flags are in this prefix.
- The player actor initialization path at `0x1A15A8` calls `0xA9C60` **before**
  setting save deployment bit 18 at `0x1A15B0`. Consequently the copied actor's
  save flags are not sufficient to identify participation.
- Actor byte `+0xD6` bit 0 participates in writeback; byte `+0xF1` is side
  (player initialization passes 0). Adjutant initialization at `0xA9F40`
  copies its save record and sets `+0xD6` bit 5 at `0xAA060`.
- GOT `0x1AA8078` points to the actor-registry pointer. The registry has 105
  actor pointers at `+8`; the vanilla writeback loop iterates those slots.
- `0x96690` is called when the battle manager finishes its exit state. GOT
  `0x1AA77A8` points to that manager. Its word `+0x3D00` selects exit processing:
  mode 0 calls `0x96B40`; mode 5 calls the separate rollback path `0x967F0`;
  mode 3 has separate event/rewind handling. Only mode 0 grants catch-up.
- `0x96B40` looks up each participating actor's save record by character ID
  (`0x3CAF30`) and calls `0x415330` at `0x96D58`.
- `0x415330(save, actor)` copies `0xC4` bytes back, then updates save-only
  mirrored fields. This includes adjutant levels and EXP.
- After writeback, `0x96690` finishes save roster cleanup. Catch-up runs only
  **after the entire original `0x96690` returns**, using freshly read save
  levels and the player fighter IDs frozen from actors before the call.

No battle actor pointers are retained across the original exit call. The
active save pointer is checked again afterward. Empty, missing or duplicate
fighter identities fail closed. Critical hook, role-field and writeback
instructions are guarded before installing either hook.

## Regression and remaining verification

Host tests cover stale copied flags, other sides, adjutants, final levels,
record reordering, cleared deployment flags, missing/duplicate records,
unavailable units, repeated exits, retreat and the next battle.

The supplied slot00 levels give ten fighters totaling 154: average 15, floor
12 with gap 3. Mercedes at level 10 needs catch-up; ordinary bench units
already at 14 do not. This is a regression fixture, not a prediction of the
levels after the next battle.

Static disassembly and host tests do not confirm hardware/emulator execution.
Required acceptance: finish a normal battle, leave the result screens, check
Mercedes and the bench, manually save, then reload and confirm the changes.
New logs distinguish battle actor capture, settled save before catch-up,
EXP grants, and save records after catch-up (`diagnostic_schema=2`).
