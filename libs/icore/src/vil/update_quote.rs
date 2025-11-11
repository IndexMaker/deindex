use deli::{amount::*, vis::*};
use devil_macros::devil;

/// Update Index Quote (Capacity, Price, Slope)
/// 
pub fn update_quote(
    _index_asset_names_id: u128,
    _weights_id: u128,
    _quote_id: u128,
    _inventory_asset_names_id: u128,
    _asset_prices_id: u128,
    _asset_slopes_id: u128,
    _asset_liquidity: u128,
    _delta_long_id: u128,
    _delta_short_id: u128,
) -> Vec<u128> {
    devil! {
        // TODO: Write implementation
    }
}
