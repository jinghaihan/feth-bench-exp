pub const TITLE_ID: u64 = 0x0100_55D0_09F7_8000;
pub const DISPLAY_VERSION: &[u8] = b"1.2.0";
pub const SAVE_POINTER_OFFSET: usize = 0x01B1_2190;
pub const EXP_TABLE_POINTER_OFFSET: usize = 0x01AA_90C0;
pub const ADD_EXP_OFFSET: usize = 0x0041_1400;
pub const BATTLE_ACTORS_INIT_OFFSET: usize = 0x001A_1440;
pub const BATTLE_EXIT_OFFSET: usize = 0x0009_6690;
pub const BATTLE_MANAGER_GOT_OFFSET: usize = 0x01AA_77A8;
pub const BATTLE_UNITS_GOT_OFFSET: usize = 0x01AA_8078;
pub const BATTLE_UNIT_COUNT: usize = 105;
pub const BATTLE_EXIT_MODE_OFFSET: usize = 0x3D00;
pub const SAVE_UNIT_BASE: usize = 0x644;
pub const SAVE_UNIT_STRIDE: usize = 0x24C;
pub const SAVE_UNIT_COUNT: usize = 60;

// Original AArch64 instructions from main NSO
// Build ID 89048449BA238C8CF565518B83BF02D3.
pub const TEXT_SIGNATURES: [(usize, u32); 19] = [
  (BATTLE_ACTORS_INIT_OFFSET, 0xA9BA_6FFC),
  (0x001A_1458, 0x9101_43FD),
  (BATTLE_EXIT_OFFSET, 0xA9BC_5FF8),
  (0x0009_66AC, 0xB97D_0268),
  (0x0009_66D0, 0x7100_151F),
  (0x0009_66E4, 0x9400_0117),
  (0x0009_6724, 0x3900_0317),
  (0x0009_6B40, 0xA9BA_6FFC),
  (0x0009_6BB4, 0x3943_5A88),
  (0x0009_6D58, 0x940D_F976),
  (0x0009_6C88, 0x3943_C688),
  (0x000A_9CC4, 0x3903_C674),
  (0x000A_A060, 0x321B_0108),
  (0x000A_A064, 0x3903_5A88),
  (0x0041_5350, 0x941B_9BCC),
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
    for (changed, _) in TEXT_SIGNATURES {
      assert!(!matches_text_signatures(|offset| {
        if offset == changed {
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
}
