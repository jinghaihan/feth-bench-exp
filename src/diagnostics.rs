use std::{
  fmt,
  sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
  },
};

use skyline::nn::fs;

use crate::config;

// Keep this mount distinct from other plugins, including the durability logger.
const MOUNT: &[u8] = b"fethbench\0";
const CONFIG_PATH: &[u8] = b"fethbench:/feth-bench-exp.cfg\0";
const LOG_PATH: &[u8] = b"fethbench:/feth-bench-exp.log\0";
const MAX_CONFIG_BYTES: i64 = 4096;
const MAX_LOG_BYTES: i64 = 2 * 1024 * 1024;
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

pub fn init() {
  // SAFETY: all filesystem paths and the mount name are NUL-terminated.
  let result = unsafe { fs::MountSdCard(MOUNT.as_ptr()) };
  if result != 0 {
    println!("[feth-bench-exp] diagnostic SD mount failed: {result:#x}");
    return;
  }
  let Some(contents) = read_config() else {
    return;
  };
  match config::diagnostic_log_enabled(&contents) {
    Ok(true) => {}
    Ok(false) => return,
    Err(error) => {
      println!("[feth-bench-exp] invalid diagnostic configuration: {error}");
      return;
    }
  }
  // Preserve previous sessions; CreateFile may report that the file exists.
  unsafe { fs::CreateFile(LOG_PATH.as_ptr(), 0) };
  ENABLED.store(true, Ordering::Release);
  log!(
    "=== feth bench-exp diagnostic; new game launch; diagnostic_schema=1; plugin_version={} ===",
    env!("CARGO_PKG_VERSION")
  );
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
  let mut handle = fs::FileHandle { handle: 0 };
  // SAFETY: use a valid opened handle and keep the line alive until WriteFile returns.
  unsafe {
    let result = fs::OpenFile(
      &mut handle,
      LOG_PATH.as_ptr(),
      (fs::OpenMode_OpenMode_Write | fs::OpenMode_OpenMode_Append) as i32,
    );
    if result != 0 {
      ENABLED.store(false, Ordering::Release);
      println!("[feth-bench-exp] diagnostic log open failed: {result:#x}");
      return;
    }
    let mut offset = 0i64;
    let size_result = fs::GetFileSize(&mut offset, handle);
    if size_result != 0 || offset < 0 {
      fs::CloseFile(handle);
      ENABLED.store(false, Ordering::Release);
      println!("[feth-bench-exp] diagnostic log size failed: {size_result:#x}");
      return;
    }
    let line = format!("{message}\n");
    if offset + line.len() as i64 > MAX_LOG_BYTES {
      fs::CloseFile(handle);
      ENABLED.store(false, Ordering::Release);
      println!("[feth-bench-exp] diagnostic log reached 2 MiB; logging disabled");
      return;
    }
    let option = fs::WriteOption {
      flags: fs::WriteOptionFlag_WriteOptionFlag_Flush as i32,
    };
    let result = fs::WriteFile(handle, offset, line.as_ptr(), line.len() as u64, &option);
    fs::CloseFile(handle);
    if result != 0 {
      ENABLED.store(false, Ordering::Release);
      println!("[feth-bench-exp] diagnostic log write failed: {result:#x}");
    }
  }
}
