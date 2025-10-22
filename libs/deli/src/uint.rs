use alloc::vec::Vec;

#[inline]
pub fn read_u128(input: &[u8]) -> u128 {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&input[0..16]);
    u128::from_le_bytes(bytes)
}

#[inline]
pub fn write_u128(input: u128, output: &mut Vec<u8>) {
    let bytes = input.to_le_bytes();
    output.extend_from_slice(&bytes);
}
