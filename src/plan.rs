//! Side-effect-free selection of a victory's catch-up recipients.

use core::sync::atomic::{AtomicBool, Ordering};

pub const LEVEL_GAP: u8 = 3;
pub const MAX_LEVEL: u8 = 99;

const AVAILABLE: u32 = 1 << 0;
const JOINED: u32 = 1 << 1;
const UNAVAILABLE_OR_DEAD: u32 = (1 << 2) | (1 << 3);
const DEPLOYED: u32 = 1 << 18;
const ADJUTANT: u32 = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSnapshot {
  pub character: i16,
  pub level: u8,
  pub flags: u32,
}

/// Battle actors' participation, side and adjutant state are authoritative.
/// Save deployment flags can be set after the actor was copied from the save.
pub fn battle_snapshot(unit: UnitSnapshot, state: u8, side: u8) -> Option<UnitSnapshot> {
  if state & 1 == 0 || side != 0 {
    return None;
  }
  Some(UnitSnapshot {
    flags: (unit.flags & !(DEPLOYED | ADJUTANT))
      | DEPLOYED
      | if state & 0x20 != 0 { ADJUTANT } else { 0 },
    ..unit
  })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatchUp {
  pub character: i16,
  pub from_level: u8,
  pub target_level: u8,
}

/// Armed by battle actor initialization; consumed even when exiting by retreat.
pub struct BattleCycle(AtomicBool);

impl BattleCycle {
  pub const fn new() -> Self {
    Self(AtomicBool::new(false))
  }

  pub fn begin(&self) {
    self.0.store(true, Ordering::Release);
  }

  pub fn finish(&self, mode: Option<u32>) -> bool {
    self.0.swap(false, Ordering::AcqRel) && mode == Some(0)
  }
}

impl Default for BattleCycle {
  fn default() -> Self {
    Self::new()
  }
}

/// IDs, not save pointers or levels: cleanup changes flags and writes battle
/// copies back into the save. Only the settled levels belong in the average.
#[derive(Debug)]
pub struct BattleRoster {
  fighters: Vec<i16>,
  adjutants: Vec<i16>,
}

impl BattleRoster {
  pub fn capture(units: &[UnitSnapshot]) -> Option<Self> {
    let mut seen = Vec::new();
    for unit in units.iter().filter(|unit| eligible(**unit)) {
      if seen.contains(&unit.character) {
        return None;
      }
      seen.push(unit.character);
    }
    let fighters: Vec<_> = units
      .iter()
      .copied()
      .filter(|unit| formally_deployed(*unit))
      .map(|unit| unit.character)
      .collect();
    if fighters.is_empty() {
      return None;
    }
    let adjutants = units
      .iter()
      .filter(|unit| eligible(**unit) && unit.flags & ADJUTANT != 0)
      .map(|unit| unit.character)
      .collect();
    Some(Self {
      fighters,
      adjutants,
    })
  }

  pub fn fighters(&self) -> &[i16] {
    &self.fighters
  }

  pub fn is_adjutant(&self, character: i16) -> bool {
    self.adjutants.contains(&character)
  }

  pub fn plan(&self, settled: &[UnitSnapshot], gap: u8) -> (Option<u8>, Vec<CatchUp>) {
    // Missing or duplicate records must not silently shrink the average.
    for character in &self.fighters {
      if settled
        .iter()
        .filter(|unit| unit.character == *character)
        .count()
        != 1
      {
        return (None, Vec::new());
      }
    }
    for (index, unit) in settled
      .iter()
      .enumerate()
      .filter(|(_, unit)| eligible(**unit))
    {
      if settled[..index]
        .iter()
        .any(|earlier| earlier.character == unit.character)
      {
        return (None, Vec::new());
      }
    }
    let normalized: Vec<_> = settled
      .iter()
      .map(|unit| UnitSnapshot {
        flags: (unit.flags & !(DEPLOYED | ADJUTANT))
          | if self.fighters.contains(&unit.character) {
            DEPLOYED
          } else {
            0
          },
        ..*unit
      })
      .collect();
    catch_up_plan(&normalized, gap)
  }
}

pub fn eligible(unit: UnitSnapshot) -> bool {
  unit.character >= 0
    && (1..=MAX_LEVEL).contains(&unit.level)
    && unit.flags & (AVAILABLE | JOINED) == AVAILABLE | JOINED
    && unit.flags & UNAVAILABLE_OR_DEAD == 0
}

pub fn diagnostic_status(unit: UnitSnapshot, floor: Option<u8>) -> &'static str {
  if unit.character < 0 {
    "empty_slot"
  } else if !(1..=MAX_LEVEL).contains(&unit.level) {
    "invalid_level"
  } else if unit.flags & JOINED == 0 {
    "not_joined"
  } else if unit.flags & AVAILABLE == 0 {
    "not_available"
  } else if unit.flags & UNAVAILABLE_OR_DEAD != 0 {
    "unavailable_or_dead"
  } else if formally_deployed(unit) {
    "deployed_average_member"
  } else if let Some(floor) = floor {
    if unit.level >= floor {
      "at_or_above_floor"
    } else if unit.flags & ADJUTANT != 0 {
      "catch_up_adjutant"
    } else {
      "catch_up_bench"
    }
  } else {
    "no_deployed_average"
  }
}

pub fn formally_deployed(unit: UnitSnapshot) -> bool {
  eligible(unit) && unit.flags & DEPLOYED != 0 && unit.flags & ADJUTANT == 0
}

pub fn catch_up_plan(units: &[UnitSnapshot], level_gap: u8) -> (Option<u8>, Vec<CatchUp>) {
  let (level_sum, fighters) = units
    .iter()
    .copied()
    .filter(|unit| formally_deployed(*unit))
    .fold((0u32, 0u32), |(sum, count), unit| {
      (sum + u32::from(unit.level), count + 1)
    });
  if fighters == 0 {
    return (None, Vec::new());
  }

  let average = level_sum / fighters;
  let floor = average.saturating_sub(u32::from(level_gap)) as u8;
  let recipients = units
    .iter()
    .copied()
    .filter(|unit| eligible(*unit) && !formally_deployed(*unit) && unit.level < floor)
    .map(|unit| CatchUp {
      character: unit.character,
      from_level: unit.level,
      target_level: floor,
    })
    .collect();
  (Some(floor), recipients)
}

#[cfg(test)]
mod tests {
  use super::*;

  const ACTIVE: u32 = AVAILABLE | JOINED;

  fn unit(character: i16, level: u8, extra_flags: u32) -> UnitSnapshot {
    UnitSnapshot {
      character,
      level,
      flags: ACTIVE | extra_flags,
    }
  }

  #[test]
  fn actual_battle_roles_override_stale_save_flags_and_exclude_other_sides() {
    assert_eq!(
      battle_snapshot(unit(1, 18, 0), 3, 0),
      Some(unit(1, 18, DEPLOYED))
    );
    assert_eq!(
      battle_snapshot(unit(16, 10, 0), 0x23, 0),
      Some(unit(16, 10, DEPLOYED | ADJUTANT))
    );
    assert_eq!(
      battle_snapshot(unit(1, 18, ADJUTANT), 3, 0),
      Some(unit(1, 18, DEPLOYED))
    );
    assert_eq!(battle_snapshot(unit(1, 18, DEPLOYED), 0, 0), None);
    assert_eq!(battle_snapshot(unit(1, 18, DEPLOYED), 3, 1), None);
    assert_eq!(battle_snapshot(unit(1, 18, DEPLOYED), 3, 2), None);
  }

  #[test]
  fn settled_save_uses_frozen_ids_not_cleared_deployment_flags() {
    // Latest slot00: ten fighters total 154, Mercedes Lv10, Ingrid Lv12,
    // Leonie Lv14, ordinary bench Lv14. The correct floor is 12, not 15.
    let levels = [18, 15, 14, 15, 16, 15, 15, 15, 16, 15];
    let mut battle: Vec<_> = levels
      .iter()
      .enumerate()
      .map(|(index, level)| unit(index as i16, *level, DEPLOYED))
      .collect();
    battle.extend([
      unit(16, 10, DEPLOYED | ADJUTANT),
      unit(18, 12, DEPLOYED | ADJUTANT),
      unit(25, 14, DEPLOYED | ADJUTANT),
      unit(40, 14, 0),
      unit(41, 11, 0),
    ]);
    let roster = BattleRoster::capture(&battle).unwrap();
    let settled: Vec<_> = battle
      .iter()
      .map(|u| UnitSnapshot {
        flags: ACTIVE | if u.character == 0 { DEPLOYED } else { 0 },
        ..*u
      })
      .collect();
    assert_eq!(catch_up_plan(&settled, 3).0, Some(15));
    let (floor, recipients) = roster.plan(&settled, 3);
    assert_eq!(floor, Some(12));
    assert_eq!(
      recipients,
      [
        CatchUp {
          character: 16,
          from_level: 10,
          target_level: 12
        },
        CatchUp {
          character: 41,
          from_level: 11,
          target_level: 12
        },
      ]
    );
    assert!(roster.is_adjutant(16));
    assert!(!roster.is_adjutant(40));
  }

  #[test]
  fn uses_final_levels_after_battle_copy_writeback_and_slot_reordering() {
    let before = [
      unit(1, 20, DEPLOYED),
      unit(2, 20, DEPLOYED),
      unit(3, 9, DEPLOYED | ADJUTANT),
      unit(4, 8, 0),
    ];
    let roster = BattleRoster::capture(&before).unwrap();
    let settled = [
      unit(4, 8, 0),
      unit(3, 18, 0),
      unit(2, 24, 0),
      unit(1, 22, 0),
    ];
    let (floor, recipients) = roster.plan(&settled, 3);
    assert_eq!(floor, Some(20));
    assert_eq!(
      recipients,
      [
        CatchUp {
          character: 4,
          from_level: 8,
          target_level: 20
        },
        CatchUp {
          character: 3,
          from_level: 18,
          target_level: 20
        },
      ]
    );
    let mut caught_up = settled;
    caught_up[0].level = 20;
    caught_up[1].level = 20;
    assert_eq!(roster.plan(&caught_up, 3), (Some(20), vec![]));
  }

  #[test]
  fn frozen_fighters_never_become_recipients_even_when_below_average() {
    let battle = [
      unit(1, 30, DEPLOYED),
      unit(2, 10, DEPLOYED),
      unit(3, 5, ADJUTANT),
    ];
    let roster = BattleRoster::capture(&battle).unwrap();
    let settled = battle.map(|u| UnitSnapshot { flags: ACTIVE, ..u });
    let (_, recipients) = roster.plan(&settled, 0);
    assert_eq!(
      recipients,
      [CatchUp {
        character: 3,
        from_level: 5,
        target_level: 20
      }]
    );
  }

  #[test]
  fn missing_or_duplicate_fighters_abort_instead_of_using_only_byleth() {
    let battle = [unit(1, 18, DEPLOYED), unit(2, 14, DEPLOYED), unit(3, 10, 0)];
    let roster = BattleRoster::capture(&battle).unwrap();
    assert_eq!(roster.plan(&[battle[0], battle[2]], 3), (None, vec![]));
    assert_eq!(
      roster.plan(&[battle[0], battle[1], battle[1]], 3),
      (None, vec![])
    );
    assert_eq!(
      roster.plan(&[battle[0], battle[1], battle[2], battle[2]], 3),
      (None, vec![])
    );
    assert!(BattleRoster::capture(&[battle[0], battle[0]]).is_none());
    assert!(BattleRoster::capture(&[battle[2]]).is_none());
  }

  #[test]
  fn unavailable_units_remain_excluded_after_settlement() {
    let battle = [
      unit(1, 20, DEPLOYED),
      unit(2, 18, DEPLOYED),
      unit(3, 10, ADJUTANT),
    ];
    let roster = BattleRoster::capture(&battle).unwrap();
    let settled = [unit(1, 20, 0), unit(2, 18, 1 << 3), unit(3, 10, 1 << 2)];
    assert_eq!(roster.plan(&settled, 3), (Some(17), vec![]));
  }

  #[test]
  fn each_battle_can_finish_only_once_including_retreat_and_next_battle() {
    let cycle = BattleCycle::new();
    assert!(!cycle.finish(Some(0)));
    cycle.begin();
    cycle.begin(); // Initialization can be repeated during map loading.
    assert!(cycle.finish(Some(0)));
    for _ in 0..500 {
      assert!(!cycle.finish(Some(0)));
    }
    cycle.begin();
    assert!(!cycle.finish(Some(5))); // Retreat consumes the cycle, without EXP.
    assert!(!cycle.finish(Some(0)));
    for mode in [Some(3), Some(1), None] {
      cycle.begin();
      assert!(!cycle.finish(mode));
      assert!(!cycle.finish(Some(0)));
    }
    cycle.begin();
    assert!(cycle.finish(Some(0)));
  }

  #[test]
  fn uses_only_formal_fighters_for_average_and_includes_adjutants_as_bench() {
    let units = [
      unit(1, 25, DEPLOYED),
      unit(2, 26, DEPLOYED),
      unit(3, 24, DEPLOYED),
      unit(4, 8, DEPLOYED | ADJUTANT),
      unit(5, 12, 0),
    ];
    let (floor, recipients) = catch_up_plan(&units, 5);
    assert_eq!(floor, Some(20));
    assert_eq!(recipients.len(), 2);
    assert_eq!(recipients[0].character, 4);
    assert_eq!(recipients[1].character, 5);
    assert!(recipients
      .iter()
      .all(|recipient| recipient.target_level == 20));
  }

  #[test]
  fn excludes_dead_unavailable_unrecruited_and_fighters() {
    let mut unrecruited = unit(4, 1, 0);
    unrecruited.flags &= !JOINED;
    let mut unavailable = unit(5, 1, 0);
    unavailable.flags &= !AVAILABLE;
    let units = [
      unit(1, 30, DEPLOYED),
      unit(2, 1, 1 << 3),
      unit(3, 1, 1 << 2),
      unrecruited,
      unavailable,
      unit(6, 24, 0),
    ];
    let (floor, recipients) = catch_up_plan(&units, 5);
    assert_eq!(floor, Some(25));
    assert_eq!(recipients.len(), 1);
    assert_eq!(recipients[0].character, 6);
  }

  #[test]
  fn never_reduces_level_or_acts_without_a_fighter() {
    let units = [unit(1, 10, 0), unit(2, 12, ADJUTANT)];
    assert_eq!(catch_up_plan(&units, LEVEL_GAP), (None, vec![]));
    let units = [unit(1, 20, DEPLOYED), unit(2, 20, 0), unit(3, 15, 0)];
    assert_eq!(catch_up_plan(&units, 5), (Some(15), vec![]));
  }

  #[test]
  fn level_gap_is_configurable_and_average_rounds_down() {
    let units = [unit(1, 24, DEPLOYED), unit(2, 25, DEPLOYED), unit(3, 1, 0)];
    assert_eq!(catch_up_plan(&units, 3).0, Some(21));
    assert_eq!(catch_up_plan(&units, 7).0, Some(17));
    assert_eq!(catch_up_plan(&units, 0).0, Some(24));
    assert_eq!(catch_up_plan(&units, 99), (Some(0), vec![]));
  }

  #[test]
  fn default_gap_is_three_and_only_raises_units_below_the_floor() {
    assert_eq!(LEVEL_GAP, 3);
    let units = [unit(1, 7, DEPLOYED), unit(2, 3, 0), unit(3, 5, 0)];
    let (floor, recipients) = catch_up_plan(&units, LEVEL_GAP);
    assert_eq!(floor, Some(4));
    assert_eq!(
      recipients,
      vec![CatchUp {
        character: 2,
        from_level: 3,
        target_level: 4
      }]
    );
  }

  #[test]
  fn diagnostic_reasons_match_recipient_selection() {
    let units = [
      unit(1, 25, DEPLOYED),
      unit(2, 10, 0),
      unit(3, 10, DEPLOYED | ADJUTANT),
      unit(4, 20, 0),
      unit(5, 1, 1 << 3),
      unit(6, 1, 1 << 2),
      UnitSnapshot {
        character: -1,
        ..unit(7, 1, 0)
      },
      unit(8, 0, 0),
      UnitSnapshot {
        flags: AVAILABLE,
        ..unit(9, 1, 0)
      },
      UnitSnapshot {
        flags: JOINED,
        ..unit(10, 1, 0)
      },
    ];
    let (floor, recipients) = catch_up_plan(&units, 5);
    let statuses: Vec<_> = units
      .iter()
      .map(|unit| diagnostic_status(*unit, floor))
      .collect();
    assert_eq!(
      statuses,
      [
        "deployed_average_member",
        "catch_up_bench",
        "catch_up_adjutant",
        "at_or_above_floor",
        "unavailable_or_dead",
        "unavailable_or_dead",
        "empty_slot",
        "invalid_level",
        "not_joined",
        "not_available",
      ]
    );
    for unit in units {
      let status = diagnostic_status(unit, floor);
      assert_eq!(
        recipients
          .iter()
          .any(|recipient| recipient.character == unit.character),
        status == "catch_up_bench" || status == "catch_up_adjutant"
      );
    }
    assert_eq!(
      diagnostic_status(unit(1, 10, 0), None),
      "no_deployed_average"
    );
    assert_eq!(
      diagnostic_status(unit(1, 99, 0), Some(20)),
      "at_or_above_floor"
    );
  }

  #[test]
  fn diagnostic_status_agrees_with_all_flag_combinations() {
    for flags in 0..32 {
      for deployment in [0, DEPLOYED, ADJUTANT, DEPLOYED | ADJUTANT] {
        for character in [-1, 1, 1045] {
          for level in [0, 1, 19, 20, 99, 100] {
            let unit = UnitSnapshot {
              character,
              level,
              flags: flags | deployment,
            };
            let status = diagnostic_status(unit, Some(20));
            assert_eq!(
              status == "catch_up_bench" || status == "catch_up_adjutant",
              eligible(unit) && !formally_deployed(unit) && unit.level < 20,
              "{unit:?} -> {status}"
            );
          }
        }
      }
    }
  }

  #[test]
  fn low_deployed_average_cannot_raise_a_bench_unit() {
    let units = [unit(1, 4, DEPLOYED), unit(2, 1, 0)];
    assert_eq!(catch_up_plan(&units, LEVEL_GAP), (Some(1), vec![]));
    assert_eq!(diagnostic_status(units[1], Some(1)), "at_or_above_floor");
  }
}
