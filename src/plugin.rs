use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use crate::{
  diagnostics,
  game::{profile, runtime::Runtime},
  plan::{catch_up_plan, diagnostic_status, eligible, formally_deployed, UnitSnapshot, LEVEL_GAP},
};

static PROCESSING: AtomicBool = AtomicBool::new(false);
static STAGE_CLEAR_COUNT: AtomicUsize = AtomicUsize::new(0);
static CONFIGURED_LEVEL_GAP: AtomicU8 = AtomicU8::new(LEVEL_GAP);

pub fn install() {
  let settings = diagnostics::init();
  CONFIGURED_LEVEL_GAP.store(settings.level_gap, Ordering::Release);
  if !Runtime::initialize() {
    diagnostics::log!("hooks=not_installed reason=unsupported_or_modified_game");
    println!("[feth-bench-exp] unsupported or modified game; hook not installed");
    return;
  }
  skyline::install_hooks!(stage_clear_hook);
  diagnostics::log!(
    "hooks=installed stage_clear_offset={:#x} add_exp_offset={:#x} level_gap={}",
    profile::STAGE_CLEAR_UI_OFFSET,
    profile::ADD_EXP_OFFSET,
    settings.level_gap
  );
  println!("[feth-bench-exp] stage-clear catch-up hook installed");
}

// FE3H 1.2.0 builds the battle stage-clear UI here. This is the best static
// victory boundary found so far; in-game confirmation of timing is required.
#[skyline::hook(offset = profile::STAGE_CLEAR_UI_OFFSET)]
fn stage_clear_hook(screen: *mut core::ffi::c_void) {
  let event = STAGE_CLEAR_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
  diagnostics::log!(
    "stage_clear event={} phase=enter screen={:p}",
    event,
    screen
  );
  let acquired = PROCESSING
    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
    .is_ok();
  if acquired {
    run_catch_up(event);
    PROCESSING.store(false, Ordering::Release);
  } else {
    diagnostics::log!(
      "stage_clear event={} status=skipped reason=reentrant",
      event
    );
  }
  call_original!(screen);
  if acquired && diagnostics::enabled() {
    log_after_stage_clear(event);
  }
  diagnostics::log!("stage_clear event={} phase=exit", event);
}

fn run_catch_up(event: usize) {
  let Some(runtime) = Runtime::get() else {
    diagnostics::log!(
      "stage_clear event={} status=skipped reason=runtime_unavailable",
      event
    );
    return;
  };
  let Some(save) = runtime.save() else {
    diagnostics::log!(
      "stage_clear event={} status=skipped reason=save_pointer_null",
      event
    );
    return;
  };
  diagnostics::log!(
    "stage_clear event={} phase=before save={:p}",
    event,
    save.as_ptr()
  );

  let mut units = Vec::with_capacity(profile::SAVE_UNIT_COUNT);
  for index in 0..profile::SAVE_UNIT_COUNT {
    // SAFETY: index is bounded by SAVE_UNIT_COUNT and save is the active game
    // save pointer selected by the verified 1.2.0 executable profile.
    let Some(unit) = (unsafe { runtime.save_unit(save, index) }) else {
      diagnostics::log!(
        "stage_clear event={} status=aborted reason=unit_pointer_unavailable slot={}",
        event,
        index
      );
      return;
    };
    units.push(unsafe { Runtime::snapshot(unit) });
  }

  let level_gap = CONFIGURED_LEVEL_GAP.load(Ordering::Acquire);
  let (floor, recipients) = catch_up_plan(&units, level_gap);
  if diagnostics::enabled() {
    let (sum, fighters) = units
      .iter()
      .filter(|unit| formally_deployed(**unit))
      .fold((0u32, 0u32), |(sum, count), unit| {
        (sum + u32::from(unit.level), count + 1)
      });
    diagnostics::log!(
      "plan event={} deployed_count={} level_sum={} average={:?} gap={} floor={:?} recipients={}",
      event,
      fighters,
      sum,
      sum.checked_div(fighters),
      level_gap,
      floor,
      recipients.len()
    );
    for (index, snapshot) in units.iter().enumerate() {
      if let Some(unit) = unsafe { runtime.save_unit(save, index) } {
        diagnostics::log!(
          "unit event={} phase=before slot={} character={} level={} exp={} flags={:#010x} status={}",
          event, index, snapshot.character, snapshot.level,
          unsafe { Runtime::current_exp(unit) }, snapshot.flags, diagnostic_status(*snapshot, floor)
        );
      }
    }
  }
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
      diagnostics::log!(
        "recipient event={} character={} status=skipped reason=record_not_found",
        event,
        recipient.character
      );
      continue;
    };
    let Some(unit) = (unsafe { runtime.save_unit(save, index) }) else {
      diagnostics::log!(
        "recipient event={} slot={} status=skipped reason=unit_pointer_unavailable",
        event,
        index
      );
      continue;
    };
    // Recheck eligibility because a game callback could change the unit while
    // an earlier recipient is raised. This also prevents stale plans.
    let current: UnitSnapshot = unsafe { Runtime::snapshot(unit) };
    if !eligible(current)
      || current.character != recipient.character
      || current.level != recipient.from_level
    {
      diagnostics::log!(
        "recipient event={} slot={} character={} status=skipped reason=snapshot_changed planned_level={} current={:?}",
        event, index, recipient.character, recipient.from_level, current
      );
      continue;
    }
    diagnostics::log!(
      "recipient event={} slot={} character={} from={} target={} route=stage_clear->raise_to->add_exp",
      event, index, recipient.character, current.level, recipient.target_level
    );
    match unsafe { runtime.raise_to(unit, recipient.target_level) } {
      Ok(level) => {
        diagnostics::log!(
          "recipient event={} slot={} character={} status=raised from={} to={} exp={}",
          event,
          index,
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
          "recipient event={} slot={} character={} status=stopped reason={} after={:?} exp={}",
          event,
          index,
          recipient.character,
          reason,
          unsafe { Runtime::snapshot(unit) },
          unsafe { Runtime::current_exp(unit) }
        );
        println!(
          "[feth-bench-exp] character {}: stopped ({})",
          recipient.character, reason
        );
      }
    }
  }
}

fn log_after_stage_clear(event: usize) {
  let Some(runtime) = Runtime::get() else {
    return;
  };
  let Some(save) = runtime.save() else {
    diagnostics::log!(
      "stage_clear event={} phase=after_original reason=save_pointer_null",
      event
    );
    return;
  };
  diagnostics::log!(
    "stage_clear event={} phase=after_original save={:p}",
    event,
    save.as_ptr()
  );
  for index in 0..profile::SAVE_UNIT_COUNT {
    // SAFETY: bounded index into the validated active save's unit array.
    if let Some(unit) = unsafe { runtime.save_unit(save, index) } {
      let snapshot = unsafe { Runtime::snapshot(unit) };
      diagnostics::log!(
        "unit event={} phase=after_original slot={} character={} level={} exp={} flags={:#010x}",
        event,
        index,
        snapshot.character,
        snapshot.level,
        unsafe { Runtime::current_exp(unit) },
        snapshot.flags
      );
    }
  }
}
