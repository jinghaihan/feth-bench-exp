use std::io::{self, ErrorKind};

const COPY_BYTES: usize = 4096;

pub trait Storage {
  fn size(&mut self) -> io::Result<u64>;
  fn read_at(&mut self, offset: u64, bytes: &mut [u8]) -> io::Result<usize>;
  fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()>;
  fn truncate(&mut self, size: u64) -> io::Result<()>;
}

pub fn append(file: &mut impl Storage, line: &[u8], limit: u64) -> io::Result<()> {
  if line.len() as u64 > limit || !line.ends_with(b"\n") {
    return Err(io::Error::new(ErrorKind::InvalidInput, "invalid log line"));
  }
  let mut size = file.size()?;
  if size > limit - line.len() as u64 {
    // Reclaim about half the file to avoid copying it on every subsequent line.
    // Memory use stays fixed even for an old log larger than the new limit.
    let keep = (limit / 2).min(limit - line.len() as u64);
    let mut start = size.saturating_sub(keep);
    let mut buffer = [0; COPY_BYTES];
    if start > 0 {
      // Discard the partial first line, including a split UTF-8 character.
      while start < size {
        let count = read_chunk(file, start, size, &mut buffer)?;
        if let Some(newline) = buffer[..count].iter().position(|byte| *byte == b'\n') {
          start += newline as u64 + 1;
          break;
        }
        start += count as u64;
      }
    }
    let mut source = start;
    let mut destination = 0;
    while source < size {
      let count = read_chunk(file, source, size, &mut buffer)?;
      file.write_at(destination, &buffer[..count])?;
      source += count as u64;
      destination += count as u64;
    }
    file.truncate(destination)?;
    size = destination;
  }
  file.write_at(size, line)
}

fn read_chunk(
  file: &mut impl Storage,
  offset: u64,
  size: u64,
  buffer: &mut [u8; COPY_BYTES],
) -> io::Result<usize> {
  let count = (size - offset).min(COPY_BYTES as u64) as usize;
  let read = file.read_at(offset, &mut buffer[..count])?;
  if read == 0 || read > count {
    return Err(io::Error::new(ErrorKind::UnexpectedEof, "log read failed"));
  }
  Ok(read)
}

#[cfg(target_os = "switch")]
pub mod switch {
  use super::{io, ErrorKind, Storage};
  use skyline::nn::fs;

  pub struct File {
    handle: fs::FileHandle,
  }

  impl File {
    pub fn open(path: &[u8]) -> io::Result<Self> {
      if path.last() != Some(&0) {
        return Err(io::Error::new(
          ErrorKind::InvalidInput,
          "path is not terminated",
        ));
      }
      let mut handle = fs::FileHandle { handle: 0 };
      // SAFETY: path is NUL-terminated and the OS initializes the out-handle.
      check(unsafe {
        fs::OpenFile(
          &mut handle,
          path.as_ptr(),
          (fs::OpenMode_OpenMode_ReadWrite | fs::OpenMode_OpenMode_Append) as i32,
        )
      })?;
      Ok(Self { handle })
    }

    pub fn flush(&mut self) -> io::Result<()> {
      // SAFETY: the handle stays open until Drop.
      check(unsafe { fs::FlushFile(self.handle) })
    }
  }

  impl Storage for File {
    fn size(&mut self) -> io::Result<u64> {
      let mut size = 0;
      // SAFETY: size is a valid out-pointer and the handle is open.
      check(unsafe { fs::GetFileSize(&mut size, self.handle) })?;
      u64::try_from(size).map_err(|_| io::Error::new(ErrorKind::InvalidData, "negative file size"))
    }

    fn read_at(&mut self, offset: u64, bytes: &mut [u8]) -> io::Result<usize> {
      let mut read = 0;
      // SAFETY: bytes is writable for the requested length, and the handle is open.
      check(unsafe {
        fs::ReadFile1(
          &mut read,
          self.handle,
          file_offset(offset)?,
          bytes.as_mut_ptr(),
          bytes.len() as u64,
        )
      })?;
      if read > bytes.len() as u64 {
        return Err(io::Error::new(
          ErrorKind::InvalidData,
          "invalid read length",
        ));
      }
      Ok(read as usize)
    }

    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
      let option = fs::WriteOption { flags: 0 };
      // SAFETY: bytes outlives the synchronous write and the handle is open.
      check(unsafe {
        fs::WriteFile(
          self.handle,
          file_offset(offset)?,
          bytes.as_ptr(),
          bytes.len() as u64,
          &option,
        )
      })
    }

    fn truncate(&mut self, size: u64) -> io::Result<()> {
      // SAFETY: the handle is open with write permission.
      check(unsafe { fs::SetFileSize(self.handle, file_offset(size)?) })
    }
  }

  impl Drop for File {
    fn drop(&mut self) {
      // SAFETY: this owner closes its successfully opened handle exactly once.
      unsafe { fs::CloseFile(self.handle) };
    }
  }

  fn file_offset(value: u64) -> io::Result<i64> {
    i64::try_from(value)
      .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "file offset too large"))
  }

  fn check(result: u32) -> io::Result<()> {
    if result == 0 {
      Ok(())
    } else {
      Err(io::Error::from_raw_os_error(result as i32))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Default)]
  struct MemoryFile {
    bytes: Vec<u8>,
    fail_reads: bool,
    fail_writes: bool,
    fail_truncate: bool,
    short_reads: bool,
  }

  impl Storage for MemoryFile {
    fn size(&mut self) -> io::Result<u64> {
      Ok(self.bytes.len() as u64)
    }

    fn read_at(&mut self, offset: u64, bytes: &mut [u8]) -> io::Result<usize> {
      if self.fail_reads {
        return Err(io::Error::other("read failure"));
      }
      let start = offset as usize;
      let mut count = bytes.len().min(self.bytes.len().saturating_sub(start));
      if self.short_reads {
        count = count.min(7);
      }
      bytes[..count].copy_from_slice(&self.bytes[start..start + count]);
      Ok(count)
    }

    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
      if self.fail_writes {
        return Err(io::Error::other("write failure"));
      }
      let start = offset as usize;
      self
        .bytes
        .resize(self.bytes.len().max(start + bytes.len()), 0);
      self.bytes[start..start + bytes.len()].copy_from_slice(bytes);
      Ok(())
    }

    fn truncate(&mut self, size: u64) -> io::Result<()> {
      if self.fail_truncate {
        return Err(io::Error::other("truncate failure"));
      }
      self.bytes.truncate(size as usize);
      Ok(())
    }
  }

  #[test]
  fn preserves_logs_until_the_limit() {
    let mut file = MemoryFile::default();
    append(&mut file, b"first\n", 12).unwrap();
    append(&mut file, b"second\n", 13).unwrap();
    assert_eq!(file.bytes, b"first\nsecond\n");
  }

  #[test]
  fn repeated_rollovers_keep_complete_recent_lines() {
    let mut file = MemoryFile::default();
    for event in 0..1000 {
      let line = format!("event={event:04}\n");
      append(&mut file, line.as_bytes(), 100).unwrap();
      assert!(file.bytes.len() <= 100);
      let text = std::str::from_utf8(&file.bytes).unwrap();
      assert!(text.ends_with(&line));
      let numbers: Vec<usize> = text
        .lines()
        .map(|line| line[6..].parse().unwrap())
        .collect();
      assert_eq!(numbers.last(), Some(&event));
      assert!(numbers.windows(2).all(|pair| pair[1] == pair[0] + 1));
    }
  }

  #[test]
  fn shrinks_existing_oversized_logs_using_bounded_chunks() {
    for short_reads in [false, true] {
      let mut file = MemoryFile {
        bytes: "old record\n".repeat(10000).into_bytes(),
        short_reads,
        ..MemoryFile::default()
      };
      append(&mut file, b"latest record\n", 16384).unwrap();
      assert!(file.bytes.len() <= 16384);
      let text = std::str::from_utf8(&file.bytes).unwrap();
      assert!(text.ends_with("latest record\n"));
      assert!(text
        .lines()
        .all(|line| line == "old record" || line == "latest record"));
    }
  }

  #[test]
  fn handles_utf8_partial_lines_and_large_new_records() {
    let mut file = MemoryFile {
      bytes: "é record\n".repeat(40).into_bytes(),
      ..MemoryFile::default()
    };
    append(&mut file, b"latest\n", 101).unwrap();
    let text = std::str::from_utf8(&file.bytes).unwrap();
    assert!(text
      .lines()
      .all(|line| line == "é record" || line == "latest"));
    let line = format!("{}\n", "x".repeat(100));
    append(&mut file, line.as_bytes(), 101).unwrap();
    assert_eq!(file.bytes, line.as_bytes());
  }

  #[test]
  fn discards_an_unterminated_old_tail() {
    let mut file = MemoryFile {
      bytes: vec![b'x'; 1000],
      ..MemoryFile::default()
    };
    append(&mut file, b"latest\n", 100).unwrap();
    assert_eq!(file.bytes, b"latest\n");
  }

  #[test]
  fn rejects_bad_records_without_touching_the_file() {
    let mut file = MemoryFile::default();
    assert!(append(&mut file, b"too long\n", 3).is_err());
    assert!(append(&mut file, b"no newline", 100).is_err());
    assert!(file.bytes.is_empty());
  }

  #[test]
  fn propagates_storage_failures() {
    for operation in 0..3 {
      let mut file = MemoryFile {
        bytes: "old\n".repeat(30).into_bytes(),
        fail_reads: operation == 0,
        fail_writes: operation == 1,
        fail_truncate: operation == 2,
        ..MemoryFile::default()
      };
      assert!(append(&mut file, b"latest\n", 100).is_err());
    }
  }
}
