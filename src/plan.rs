//! Side-effect-free selection of a victory's catch-up recipients.

pub const LEVEL_GAP: u8 = 5;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatchUp {
  pub character: i16,
  pub from_level: u8,
  pub target_level: u8,
}

pub fn eligible(unit: UnitSnapshot) -> bool {
  unit.character >= 0
    && (1..=MAX_LEVEL).contains(&unit.level)
    && unit.flags & (AVAILABLE | JOINED) == AVAILABLE | JOINED
    && unit.flags & UNAVAILABLE_OR_DEAD == 0
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
  fn uses_only_formal_fighters_for_average_and_includes_adjutants_as_bench() {
    let units = [
      unit(1, 25, DEPLOYED),
      unit(2, 26, DEPLOYED),
      unit(3, 24, DEPLOYED),
      unit(4, 8, DEPLOYED | ADJUTANT),
      unit(5, 12, 0),
    ];
    let (floor, recipients) = catch_up_plan(&units, LEVEL_GAP);
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
    let (floor, recipients) = catch_up_plan(&units, LEVEL_GAP);
    assert_eq!(floor, Some(25));
    assert_eq!(recipients.len(), 1);
    assert_eq!(recipients[0].character, 6);
  }

  #[test]
  fn never_reduces_level_or_acts_without_a_fighter() {
    let units = [unit(1, 10, 0), unit(2, 12, ADJUTANT)];
    assert_eq!(catch_up_plan(&units, LEVEL_GAP), (None, vec![]));
    let units = [unit(1, 20, DEPLOYED), unit(2, 20, 0), unit(3, 15, 0)];
    assert_eq!(catch_up_plan(&units, LEVEL_GAP), (Some(15), vec![]));
  }

  #[test]
  fn level_gap_is_configurable_and_average_rounds_down() {
    let units = [unit(1, 24, DEPLOYED), unit(2, 25, DEPLOYED), unit(3, 1, 0)];
    assert_eq!(catch_up_plan(&units, 3).0, Some(21));
    assert_eq!(catch_up_plan(&units, 7).0, Some(17));
  }
}
