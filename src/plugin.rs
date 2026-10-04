use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use crate::{
  diagnostics,
  game::{profile, runtime::Runtime},
  plan::{eligible, BattleCycle, BattleRoster, UnitSnapshot, LEVEL_GAP},
};

static PROCESSING: AtomicBool = AtomicBool::new(false);
static BATTLE: BattleCycle = BattleCycle::new();
static BATTLE_EXIT_COUNT: AtomicUsize = AtomicUsize::new(0);
static CONFIGURED_LEVEL_GAP: AtomicU8 = AtomicU8::new(LEVEL_GAP);

pub fn install() {
  let settings = diagnostics::init();
  CONFIGURED_LEVEL_GAP.store(settings.level_gap, Ordering::Release);
  if !Runtime::initialize() {
    diagnostics::log!("hooks=not_installed reason=unsupported_or_modified_game");
    println!("[feth-bench-exp] unsupported or modified game; hooks not installed");
    return;
  }
  skyline::install_hooks!(battle_actors_init_hook, battle_exit_hook);
  diagnostics::log!(
    "hooks=installed battle_init_offset={:#x} battle_exit_offset={:#x} add_exp_offset={:#x} level_gap={}",
    profile::BATTLE_ACTORS_INIT_OFFSET, profile::BATTLE_EXIT_OFFSET,
    profile::ADD_EXP_OFFSET, settings.level_gap
  );
  println!("[feth-bench-exp] post-writeback catch-up hooks installed");
}

#[skyline::hook(offset = profile::BATTLE_ACTORS_INIT_OFFSET)]
fn battle_actors_init_hook() {
  call_original!();
  BATTLE.begin();
  diagnostics::log!("battle_init status=armed");
}

// This exit path writes battle actors back only for mode 0, then cleans up
// deployment state. Freeze identities before it; grant EXP after it returns.
#[skyline::hook(offset = profile::BATTLE_EXIT_OFFSET)]
fn battle_exit_hook(context: *mut core::ffi::c_void) {
  let event = BATTLE_EXIT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
  let acquired = PROCESSING
    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
    .is_ok();
  let runtime = Runtime::get();
  let mode = runtime.and_then(Runtime::battle_exit_mode);
  let ready = acquired && BATTLE.finish(mode);
  diagnostics::log!(
    "battle_exit event={} phase=enter mode={:?} ready={} acquired={}",
    event,
    mode,
    ready,
    acquired
  );
  let pending = if ready {
    runtime.and_then(|runtime| {
      let save = runtime.save()?;
      let units = runtime.battle_snapshots()?;
      for unit in &units {
        diagnostics::log!(
          "battle_unit event={} character={} level={} flags={:#010x}",
          event,
          unit.character,
          unit.level,
          unit.flags
        );
      }
      Some((save, BattleRoster::capture(&units)?))
    })
  } else {
    None
  };
  call_original!(context);
  if let (Some(runtime), Some((save, roster))) = (runtime, pending) {
    if runtime.save() == Some(save) {
      run_catch_up(runtime, event, &roster);
    } else {
      diagnostics::log!(
        "battle_exit event={} status=skipped reason=save_pointer_changed",
        event
      );
    }
  } else {
    diagnostics::log!(
      "battle_exit event={} status=skipped reason=no_valid_normal_exit_roster",
      event
    );
  }
  if acquired {
    PROCESSING.store(false, Ordering::Release);
  }
  diagnostics::log!("battle_exit event={} phase=exit", event);
}

fn run_catch_up(runtime: Runtime, event: usize, roster: &BattleRoster) {
  let Some(save) = runtime.save() else {
    return;
  };
  let mut units = Vec::with_capacity(profile::SAVE_UNIT_COUNT);
  for index in 0..profile::SAVE_UNIT_COUNT {
    // SAFETY: the verified active save contains 60 complete unit records.
    let Some(unit) = (unsafe { runtime.save_unit(save, index) }) else {
      return;
    };
    units.push(unsafe { Runtime::snapshot(unit) });
  }
  let gap = CONFIGURED_LEVEL_GAP.load(Ordering::Acquire);
  let (floor, recipients) = roster.plan(&units, gap);
  let (sum, fighters) = units
    .iter()
    .filter(|unit| eligible(**unit) && roster.fighters().contains(&unit.character))
    .fold((0u32, 0u32), |(sum, count), unit| {
      (sum + u32::from(unit.level), count + 1)
    });
  diagnostics::log!(
    "plan event={} source=settled_save frozen_fighters={:?} deployed_count={} level_sum={} average={:?} gap={} floor={:?} recipients={}",
    event, roster.fighters(), fighters, sum, sum.checked_div(fighters), gap, floor, recipients.len()
  );
  log_roster(runtime, event, "settled_before_catch_up");
  let Some(floor) = floor else {
    diagnostics::log!(
      "battle_exit event={} status=skipped reason=invalid_settled_roster",
      event
    );
    return;
  };
  println!(
    "[feth-bench-exp] battle exit: floor {}, {} recipients",
    floor,
    recipients.len()
  );
  for recipient in recipients {
    if runtime.save() != Some(save) {
      diagnostics::log!(
        "battle_exit event={} status=aborted reason=save_pointer_changed",
        event
      );
      break;
    }
    let Some(index) = units
      .iter()
      .position(|unit| unit.character == recipient.character)
    else {
      continue;
    };
    let Some(unit) = (unsafe { runtime.save_unit(save, index) }) else {
      continue;
    };
    let current: UnitSnapshot = unsafe { Runtime::snapshot(unit) };
    if !eligible(current)
      || current.character != recipient.character
      || current.level != recipient.from_level
      || roster.fighters().contains(&current.character)
    {
      diagnostics::log!(
        "recipient event={} character={} status=skipped reason=snapshot_changed current={:?}",
        event,
        recipient.character,
        current
      );
      continue;
    }
    diagnostics::log!("recipient event={} slot={} character={} role={} from={} target={} route=post_writeback->raise_to->add_exp",
      event, index, recipient.character,
      if roster.is_adjutant(recipient.character) { "adjutant" } else { "bench" },
      current.level, recipient.target_level);
    match unsafe { runtime.raise_to(unit, recipient.target_level) } {
      Ok(level) => {
        diagnostics::log!(
          "recipient event={} character={} status=raised from={} to={} exp={}",
          event,
          recipient.character,
          recipient.from_level,
          level,
          unsafe { Runtime::current_exp(unit) }
        );
        println!(
          "[feth-bench-exp] character {}: {} -> {}",
          recipient.character, recipient.from_level, level
        );
      }
      Err(reason) => {
        diagnostics::log!(
          "recipient event={} character={} status=stopped reason={} after={:?}",
          event,
          recipient.character,
          reason,
          unsafe { Runtime::snapshot(unit) }
        );
        println!(
          "[feth-bench-exp] character {}: stopped ({})",
          recipient.character, reason
        );
      }
    }
  }
  log_roster(runtime, event, "after_catch_up");
}

fn log_roster(runtime: Runtime, event: usize, phase: &str) {
  if !diagnostics::enabled() {
    return;
  }
  let Some(save) = runtime.save() else {
    return;
  };
  for index in 0..profile::SAVE_UNIT_COUNT {
    // SAFETY: bounded index into the verified save's unit array.
    if let Some(unit) = unsafe { runtime.save_unit(save, index) } {
      let snapshot = unsafe { Runtime::snapshot(unit) };
      diagnostics::log!(
        "unit event={} phase={} slot={} character={} level={} exp={} flags={:#010x}",
        event,
        phase,
        index,
        snapshot.character,
        snapshot.level,
        unsafe { Runtime::current_exp(unit) },
        snapshot.flags
      );
    }
  }
}
