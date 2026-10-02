#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
  pub diagnostic_log: bool,
  pub log_max_kib: usize,
  pub level_gap: u8,
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      diagnostic_log: false,
      log_max_kib: 2048,
      level_gap: crate::plan::LEVEL_GAP,
    }
  }
}

pub fn parse(contents: &str) -> Result<Settings, &'static str> {
  let mut settings = Settings::default();
  let mut seen = [false; 3];
  for line in contents.lines() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let (key, value) = line.split_once('=').ok_or("expected key=value")?;
    let index = match key.trim() {
      "diagnostic_log" => 0,
      "log_max_kib" => 1,
      "level_gap" => 2,
      _ => return Err("unknown configuration key"),
    };
    if seen[index] {
      return Err("duplicate configuration key");
    }
    seen[index] = true;
    let value = value.trim();
    match index {
      0 => {
        settings.diagnostic_log = match value {
          "true" => true,
          "false" => false,
          _ => return Err("diagnostic_log must be true or false"),
        };
      }
      1 => {
        settings.log_max_kib = value.parse().map_err(|_| "invalid log_max_kib")?;
        if !(64..=65536).contains(&settings.log_max_kib) {
          return Err("log_max_kib must be between 64 and 65536");
        }
      }
      2 => {
        settings.level_gap = value.parse().map_err(|_| "invalid level_gap")?;
        if settings.level_gap > crate::plan::MAX_LEVEL {
          return Err("level_gap must be between 0 and 99");
        }
      }
      _ => unreachable!(),
    }
  }
  Ok(settings)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn logging_is_opt_in() {
    assert_eq!(parse(""), Ok(Settings::default()));
    assert_eq!(parse("# comment\n"), Ok(Settings::default()));
    assert!(parse("diagnostic_log=true\n").unwrap().diagnostic_log);
    assert!(!parse(" diagnostic_log = false\r\n").unwrap().diagnostic_log);
  }

  #[test]
  fn configures_gap_even_without_logging() {
    let settings = parse("level_gap=7\nlog_max_kib=1024\n").unwrap();
    assert_eq!(settings.level_gap, 7);
    assert_eq!(settings.log_max_kib, 1024);
    assert!(!settings.diagnostic_log);
  }

  #[test]
  fn accepts_boundary_values_and_legacy_configuration() {
    for gap in [0, 99] {
      assert_eq!(parse(&format!("level_gap={gap}")).unwrap().level_gap, gap);
    }
    for size in [64, 65536] {
      assert_eq!(
        parse(&format!("log_max_kib={size}")).unwrap().log_max_kib,
        size
      );
    }
    assert_eq!(parse("diagnostic_log=true").unwrap().level_gap, 3);
  }

  #[test]
  fn rejects_ambiguous_or_invalid_settings() {
    for contents in [
      "diagnostic_log=yes",
      "diagnostic_log=true\ndiagnostic_log=false",
      "other=true",
      "diagnostic_log",
      "level_gap=-1",
      "level_gap=100",
      "level_gap=256",
      "level_gap=3.5",
      "level_gap=3\nlevel_gap=5",
      "log_max_kib=63",
      "log_max_kib=65537",
      "log_max_kib=0",
      "log_max_kib=-1",
      "log_max_kib=huge",
      "log_max_kib=64\nlog_max_kib=128",
    ] {
      assert!(parse(contents).is_err(), "{contents}");
    }
  }
}
