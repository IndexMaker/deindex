pub const SIDE_LONG: u128 = 0b01;
pub const SIDE_SHORT: u128 = 0b10;
pub const SIDE_MASK: u128 = 0b11;
pub const SIDE_FLAT: u128 = 0b00;

pub fn get_side(asset: u128) -> u128 {
    asset & SIDE_MASK
}

pub fn get_asset_id(asset: u128) -> u128 {
    asset & !SIDE_MASK
}
