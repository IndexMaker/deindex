use devil_macros::devil;

/// Update Market (Supply, Delta)
/// 
pub fn update_supply(
    market_asset_names_id: u128,
    _supply_long_id: u128,
    _supply_short_id: u128,
    _demand_long_id: u128,
    _demand_short_id: u128,
    _delta_long_id: u128,
    _delta_short_id: u128,
) -> Vec<u128> {
    devil! {
        // TODO: Write implementation
        LDV  market_asset_names_id
    }
}