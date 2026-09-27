use core::sync::atomic::{AtomicBool, Ordering};

use crate::{
  game::{profile, runtime::Runtime},
  plan::{catch_up_plan, eligible, UnitSnapshot, LEVEL_GAP},
};

static PROCESSING: AtomicBool = AtomicBool::new(false);

pub fn install() {
  if !Runtime::initialize() {
    println!("[feth-bench-exp] unsupported or modified game; hook not installed");
    return;
  }
  skyline::install_hooks!(stage_clear_hook);
  println!("[feth-bench-exp] stage-clear catch-up hook installed");
}

// FE3H 1.2.0 builds the battle stage-clear UI here. This is the best static
// victory boundary found so far; in-game confirmation of timing is required.
#[skyline::hook(offset = profile::STAGE_CLEAR_UI_OFFSET)]
fn stage_clear_hook(screen: *mut core::ffi::c_void) {
  if PROCESSING
    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
    .is_ok()
  {
    run_catch_up();
    PROCESSING.store(false, Ordering::Release);
  }
  call_original!(screen);
}

fn run_catch_up() {
  let Some(runtime) = Runtime::get() else {
    return;
  };
  let Some(save) = runtime.save() else {
    return;
  };

  let mut units = Vec::with_capacity(profile::SAVE_UNIT_COUNT);
  for index in 0..profile::SAVE_UNIT_COUNT {
    // SAFETY: index is bounded by SAVE_UNIT_COUNT and save is the active game
    // save pointer selected by the verified 1.2.0 executable profile.
    let Some(unit) = (unsafe { runtime.save_unit(save, index) }) else {
      return;
    };
    units.push(unsafe { Runtime::snapshot(unit) });
  }

  let (floor, recipients) = catch_up_plan(&units, LEVEL_GAP);
  let Some(floor) = floor else {
    println!("[feth-bench-exp] stage clear: no active deployed units; skipped");
    return;
  };
  println!(
    "[feth-bench-exp] stage clear: floor {}, {} recipients",
    floor,
    recipients.len()
  );

  for recipient in recipients {
    let Some(index) = units
      .iter()
      .position(|unit| unit.character == recipient.character)
    else {
      continue;
    };
    let Some(unit) = (unsafe { runtime.save_unit(save, index) }) else {
      continue;
    };
    // Recheck eligibility because a game callback could change the unit while
    // an earlier recipient is raised. This also prevents stale plans.
    let current: UnitSnapshot = unsafe { Runtime::snapshot(unit) };
    if !eligible(current)
      || current.character != recipient.character
      || current.level != recipient.from_level
    {
      continue;
    }
    match unsafe { runtime.raise_to(unit, recipient.target_level) } {
      Ok(level) => println!(
        "[feth-bench-exp] character {}: {} -> {}",
        recipient.character, recipient.from_level, level
      ),
      Err(reason) => println!(
        "[feth-bench-exp] character {}: stopped ({})",
        recipient.character, reason
      ),
    }
  }
}
