use std::{
  fmt,
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
  },
};

use skyline::nn::fs;

use crate::{config, rolling_log};

// Keep this mount distinct from other plugins, including the durability logger.
const MOUNT: &[u8] = b"fethbench\0";
const CONFIG_PATH: &[u8] = b"fethbench:/feth-bench-exp.cfg\0";
const LOG_PATH: &[u8] = b"fethbench:/feth-bench-exp.log\0";
const MAX_CONFIG_BYTES: i64 = 4096;
static MAX_LOG_BYTES: AtomicUsize = AtomicUsize::new(2 * 1024 * 1024);
static ENABLED: AtomicBool = AtomicBool::new(false);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

macro_rules! log {
  ($($args:tt)*) => {
    if crate::diagnostics::enabled() {
      crate::diagnostics::append(format_args!($($args)*));
    }
  };
}
pub(crate) use log;

pub fn enabled() -> bool {
  ENABLED.load(Ordering::Acquire)
}

pub fn init() -> config::Settings {
  // SAFETY: all filesystem paths and the mount name are NUL-terminated.
  let result = unsafe { fs::MountSdCard(MOUNT.as_ptr()) };
  if result != 0 {
    println!("[feth-bench-exp] diagnostic SD mount failed: {result:#x}");
    return config::Settings::default();
  }
  let Some(contents) = read_config() else {
    return config::Settings::default();
  };
  let settings = match config::parse(&contents) {
    Ok(settings) => settings,
    Err(error) => {
      println!("[feth-bench-exp] invalid diagnostic configuration: {error}");
      return config::Settings::default();
    }
  };
  MAX_LOG_BYTES.store(settings.log_max_kib * 1024, Ordering::Release);
  if !settings.diagnostic_log {
    return settings;
  }
  // Preserve previous sessions; CreateFile may report that the file exists.
  unsafe { fs::CreateFile(LOG_PATH.as_ptr(), 0) };
  ENABLED.store(true, Ordering::Release);
  log!(
    "=== feth bench-exp diagnostic; new game launch; diagnostic_schema=1; plugin_version={}; level_gap={}; log_max_kib={} ===",
    env!("CARGO_PKG_VERSION"), settings.level_gap, settings.log_max_kib
  );
  settings
}

fn read_config() -> Option<String> {
  let mut handle = fs::FileHandle { handle: 0 };
  // SAFETY: the handle is used only after a successful open and closed on every path.
  unsafe {
    if fs::OpenFile(
      &mut handle,
      CONFIG_PATH.as_ptr(),
      fs::OpenMode_OpenMode_Read as i32,
    ) != 0
    {
      return None;
    }
    let mut size = 0i64;
    if fs::GetFileSize(&mut size, handle) != 0 || !(0..=MAX_CONFIG_BYTES).contains(&size) {
      fs::CloseFile(handle);
      println!("[feth-bench-exp] diagnostic configuration size/read failed");
      return None;
    }
    let mut bytes = vec![0u8; size as usize];
    let mut read = 0u64;
    let result = if size == 0 {
      0
    } else {
      fs::ReadFile1(&mut read, handle, 0, bytes.as_mut_ptr(), bytes.len() as u64)
    };
    fs::CloseFile(handle);
    if result != 0 || read != bytes.len() as u64 {
      println!("[feth-bench-exp] diagnostic configuration read failed: {result:#x}");
      return None;
    }
    String::from_utf8(bytes).ok()
  }
}

pub fn append(message: fmt::Arguments<'_>) {
  if !enabled() {
    return;
  }
  let Ok(_lock) = WRITE_LOCK.lock() else {
    return;
  };
  let result = (|| {
    let mut file = rolling_log::switch::File::open(LOG_PATH)?;
    let line = format!("{message}\n");
    rolling_log::append(
      &mut file,
      line.as_bytes(),
      MAX_LOG_BYTES.load(Ordering::Acquire) as u64,
    )?;
    file.flush()
  })();
  if let Err(error) = result {
    ENABLED.store(false, Ordering::Release);
    println!("[feth-bench-exp] diagnostic log failed: {error}");
  }
}
