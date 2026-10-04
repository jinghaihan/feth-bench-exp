use core::{
  ffi::c_void,
  mem,
  ptr::NonNull,
  sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
  diagnostics,
  plan::{battle_snapshot, UnitSnapshot, MAX_LEVEL},
};

use super::profile;

type AddExp = unsafe extern "C" fn(*mut c_void, i32);

static TEXT_BASE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
pub struct Runtime {
  text: NonNull<u8>,
}

impl Runtime {
  pub fn initialize() -> bool {
    let Some(runtime) = Self::detect() else {
      return false;
    };
    TEXT_BASE.store(runtime.text.as_ptr() as usize, Ordering::Release);
    true
  }

  pub fn get() -> Option<Self> {
    NonNull::new(TEXT_BASE.load(Ordering::Acquire) as *mut u8).map(|text| Self { text })
  }

  fn detect() -> Option<Self> {
    let program_id = skyline::info::get_program_id();
    diagnostics::log!(
      "program_id={:#018x} expected={:#018x}",
      program_id,
      profile::TITLE_ID
    );
    if program_id != profile::TITLE_ID {
      return None;
    }
    let mut version = skyline::nn::oe::DisplayVersion { name: [0; 16] };
    // SAFETY: the OS writes one DisplayVersion into this valid out-pointer.
    unsafe { skyline::nn::oe::GetDisplayVersion(&mut version) };
    diagnostics::log!("display_version={:?}", version.name);
    if !profile::is_supported_display_version(&version.name) {
      return None;
    }
    // SAFETY: Skyline returns the mapped main text address for this process.
    let text = NonNull::new(unsafe {
      skyline::hooks::getRegionAddress(skyline::hooks::Region::Text).cast::<u8>()
    })?;
    let runtime = Self { text };
    diagnostics::log!("text_base={:p}", text.as_ptr());
    let mut matched = true;
    for (offset, expected) in profile::TEXT_SIGNATURES {
      // SAFETY: signatures are aligned addresses inside the mapped 1.2.0 text.
      let actual = unsafe { runtime.address(offset).cast::<u32>().read_volatile() };
      diagnostics::log!(
        "signature offset={:#x} expected={:#010x} actual={:#010x} matched={}",
        offset,
        expected,
        actual,
        actual == expected
      );
      matched &= actual == expected;
    }
    if !matched {
      return None;
    }
    Some(runtime)
  }

  fn address(self, offset: usize) -> *mut u8 {
    // SAFETY: callers use fixed addresses from the validated 1.2.0 profile.
    unsafe { self.text.as_ptr().add(offset) }
  }

  fn global_pointer(self, offset: usize) -> Option<NonNull<u8>> {
    // SAFETY: these global slots were identified in the 1.2.0 main NSO.
    let pointer = unsafe { self.address(offset).cast::<*mut u8>().read() };
    NonNull::new(pointer)
  }

  pub fn save(self) -> Option<NonNull<u8>> {
    self.global_pointer(profile::SAVE_POINTER_OFFSET)
  }

  pub fn battle_exit_mode(self) -> Option<u32> {
    let manager = self.global_pointer(profile::BATTLE_MANAGER_GOT_OFFSET)?;
    // SAFETY: 0x96690 reads this same manager field before selecting normal
    // result writeback (0), rewind/event exit (3), or retreat (5).
    Some(unsafe {
      manager
        .as_ptr()
        .add(profile::BATTLE_EXIT_MODE_OFFSET)
        .cast::<u32>()
        .read_unaligned()
    })
  }

  pub fn battle_snapshots(self) -> Option<Vec<UnitSnapshot>> {
    let slot = self.global_pointer(profile::BATTLE_UNITS_GOT_OFFSET)?;
    // SAFETY: the verified writeback loop uses this registry and all 105
    // pointer slots at +8. The GOT points to the registry pointer, not records.
    let registry = NonNull::new(unsafe { slot.as_ptr().cast::<*mut u8>().read() })?;
    let mut units = Vec::new();
    for index in 0..profile::BATTLE_UNIT_COUNT {
      let pointer = unsafe {
        registry
          .as_ptr()
          .add(8 + index * 8)
          .cast::<*mut u8>()
          .read()
      };
      let Some(unit) = NonNull::new(pointer) else {
        continue;
      };
      // +D6 bit 0 participates in writeback, bit 5 is an adjutant; +F1
      // is the battle side (0 = player). These survive save flag cleanup.
      let state = unsafe { unit.as_ptr().add(0xD6).read() };
      let side = unsafe { unit.as_ptr().add(0xF1).read() };
      if let Some(snapshot) = battle_snapshot(unsafe { Self::snapshot(unit) }, state, side) {
        units.push(snapshot);
      }
    }
    Some(units)
  }

  pub unsafe fn save_unit(self, save: NonNull<u8>, index: usize) -> Option<NonNull<u8>> {
    if index >= profile::SAVE_UNIT_COUNT {
      return None;
    }
    // SAFETY: the 1.2.0 save owns 60 contiguous 0x24C-byte Unit records.
    NonNull::new(unsafe {
      save
        .as_ptr()
        .add(profile::SAVE_UNIT_BASE + index * profile::SAVE_UNIT_STRIDE)
    })
  }

  pub unsafe fn snapshot(unit: NonNull<u8>) -> UnitSnapshot {
    let pointer = unit.as_ptr();
    // SAFETY: save records and battle actors share this first 0xC4-byte layout.
    unsafe {
      UnitSnapshot {
        character: pointer.add(0x24).cast::<i16>().read_unaligned(),
        level: pointer.add(0x4A).read(),
        flags: pointer.add(0xAC).cast::<u32>().read_unaligned(),
      }
    }
  }

  unsafe fn level(unit: NonNull<u8>) -> u8 {
    // SAFETY: Unit has a level byte at +0x4A in the 1.2.0 profile.
    unsafe { unit.as_ptr().add(0x4A).read() }
  }

  pub unsafe fn current_exp(unit: NonNull<u8>) -> u16 {
    // SAFETY: Unit has a two-byte EXP field at +0x2C.
    unsafe { unit.as_ptr().add(0x2C).cast::<u16>().read_unaligned() }
  }

  fn exp_threshold(self, level: u8) -> Option<u16> {
    if level == 0 || level >= MAX_LEVEL {
      diagnostics::log!("exp_table level={} reason=invalid_level", level);
      return None;
    }
    let Some(table_slot) = self.global_pointer(profile::EXP_TABLE_POINTER_OFFSET) else {
      diagnostics::log!(
        "exp_table level={} reason=global_pointer_null offset={:#x}",
        level,
        profile::EXP_TABLE_POINTER_OFFSET
      );
      return None;
    };
    diagnostics::log!(
      "exp_table level={} phase=global table_slot={:p}",
      level,
      table_slot.as_ptr()
    );
    // SAFETY: the game's ADD_EXP routine dereferences this same global slot.
    let Some(table) = NonNull::new(unsafe { table_slot.as_ptr().cast::<*mut u8>().read() }) else {
      diagnostics::log!("exp_table level={} reason=table_pointer_null", level);
      return None;
    };
    diagnostics::log!(
      "exp_table level={} phase=table table={:p}",
      level,
      table.as_ptr()
    );
    // SAFETY: ADD_EXP reads a descriptor pointer at +0x938, then its count
    // at descriptor +4 before selecting a 24-byte table entry.
    let descriptor_slot = unsafe { table.as_ptr().add(0x938).cast::<*const u8>() };
    let Some(descriptor) = NonNull::new(unsafe { descriptor_slot.read_unaligned().cast_mut() })
    else {
      diagnostics::log!("exp_table level={} reason=descriptor_pointer_null", level);
      return None;
    };
    diagnostics::log!(
      "exp_table level={} phase=descriptor descriptor={:p}",
      level,
      descriptor.as_ptr()
    );
    let count = unsafe { descriptor.as_ptr().add(4).cast::<u32>().read_unaligned() };
    let index = usize::from(level - 1);
    if count <= index as u32 || count > 256 {
      diagnostics::log!(
        "exp_table level={} count={} index={} reason=invalid_count",
        level,
        count,
        index
      );
      return None;
    }
    // SAFETY: the table stores 24-byte entries starting at +8 with a pointer
    // at entry +8. The count above bounds the requested index.
    let entry_pointer = unsafe { table.as_ptr().add(16 + index * 24).cast::<*const u8>() };
    let Some(entry) = NonNull::new(unsafe { entry_pointer.read_unaligned().cast_mut() }) else {
      diagnostics::log!(
        "exp_table level={} count={} index={} reason=entry_pointer_null",
        level,
        count,
        index
      );
      return None;
    };
    diagnostics::log!(
      "exp_table level={} phase=entry entry={:p}",
      level,
      entry.as_ptr()
    );
    let threshold = unsafe { entry.as_ptr().cast::<u16>().read_unaligned() };
    diagnostics::log!(
      "exp_table level={} count={} index={} threshold={} valid={}",
      level,
      count,
      index,
      threshold,
      (1..=1000).contains(&threshold)
    );
    (1..=1000).contains(&threshold).then_some(threshold)
  }

  pub unsafe fn raise_to(self, unit: NonNull<u8>, target: u8) -> Result<u8, &'static str> {
    let original = unsafe { Self::level(unit) };
    if original == 0 || original > target || target > MAX_LEVEL {
      return Err("invalid level bounds");
    }
    // SAFETY: the signature gate validated the 1.2.0 function entry and ABI.
    let add_exp: AddExp = unsafe { mem::transmute(self.address(profile::ADD_EXP_OFFSET)) };
    for level in original..target {
      let threshold = self
        .exp_threshold(level)
        .ok_or("EXP threshold unavailable")?;
      let current = unsafe { Self::current_exp(unit) };
      if current > threshold {
        return Err("EXP exceeds next threshold");
      }
      let amount = i32::from(threshold - current);
      diagnostics::log!("add_exp phase=before unit={:p} offset={:#x} level={} exp={} threshold={} amount={} target={}",
        unit.as_ptr(), profile::ADD_EXP_OFFSET, level, current, threshold, amount, target);
      // SAFETY: this live Unit belongs to the save, and amount is exactly the
      // difference to the game's own next-level threshold.
      unsafe { add_exp(unit.as_ptr().cast(), amount) };
      let after_level = unsafe { Self::level(unit) };
      diagnostics::log!(
        "add_exp phase=after unit={:p} expected_level={} actual_level={} exp={}",
        unit.as_ptr(),
        level + 1,
        after_level,
        unsafe { Self::current_exp(unit) }
      );
      if after_level != level + 1 {
        return Err("vanilla EXP call did not advance exactly one level");
      }
    }
    Ok(unsafe { Self::level(unit) })
  }
}
