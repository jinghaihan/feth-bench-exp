pub const TITLE_ID: u64 = 0x0100_55D0_09F7_8000;
pub const DISPLAY_VERSION: &[u8] = b"1.2.0";
pub const SAVE_POINTER_OFFSET: usize = 0x01B1_2190;
pub const EXP_TABLE_POINTER_OFFSET: usize = 0x01AA_90C0;
pub const ADD_EXP_OFFSET: usize = 0x0041_1400;
pub const STAGE_CLEAR_UI_OFFSET: usize = 0x0014_A260;
pub const SAVE_UNIT_BASE: usize = 0x644;
pub const SAVE_UNIT_STRIDE: usize = 0x24C;
pub const SAVE_UNIT_COUNT: usize = 60;

// Original AArch64 instructions from main NSO
// Build ID 89048449BA238C8CF565518B83BF02D3.
pub const TEXT_SIGNATURES: [(usize, u32); 8] = [
  (STAGE_CLEAR_UI_OFFSET, 0xF81E_0FF3),
  (0x0014_A270, 0xF940_0E68),
  (0x0014_A29C, 0x9409_5269),
  (0x0014_A2A8, 0xD000_CAE8),
  (ADD_EXP_OFFSET, 0xA9BE_4FF4),
  (0x0041_141C, 0x7940_5A69),
  (0x0041_14A4, 0x97FF_06DF),
  (0x003C_AF30, 0x7100_043F),
];

pub fn is_supported_display_version(version: &[u8; 16]) -> bool {
  let length = version.iter().position(|byte| *byte == 0).unwrap_or(16);
  &version[..length] == DISPLAY_VERSION
}

pub fn matches_text_signatures(read_instruction: impl Fn(usize) -> u32) -> bool {
  TEXT_SIGNATURES
    .iter()
    .all(|(offset, expected)| read_instruction(*offset) == *expected)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_wrong_version_and_modified_code() {
    assert!(is_supported_display_version(b"1.2.0\0\0\0\0\0\0\0\0\0\0\0"));
    assert!(!is_supported_display_version(
      b"1.1.1\0\0\0\0\0\0\0\0\0\0\0"
    ));
    assert!(matches_text_signatures(|offset| {
      TEXT_SIGNATURES
        .iter()
        .find(|(at, _)| *at == offset)
        .unwrap()
        .1
    }));
    assert!(!matches_text_signatures(|offset| {
      if offset == STAGE_CLEAR_UI_OFFSET {
        0
      } else {
        TEXT_SIGNATURES
          .iter()
          .find(|(at, _)| *at == offset)
          .unwrap()
          .1
      }
    }));
  }
}
