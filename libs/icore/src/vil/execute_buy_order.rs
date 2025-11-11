use deli::{amount::*, vis::*};
use devil_macros::devil;

/// Execute Buy Index Order
/// 
pub fn execute_buy_order(
    order_id: u128,
    // index_asset_names: u128,
    weights_id: u128,
    quote_id: u128,
    // inventory_asset_names: u128,
    // supply_long_id: u128
    // supply_short_id: u128
    // demand_long_id: u128
    // demand_short_id: u128
    // delta_long_id: u128
    // delta_short_id: u128
    solve_quadratic_id: u128,
    order_quantities_id: u128,
) -> Vec<u128> {
    devil! {
        // Load Weights
        LDV         weights_id          // Stack: [AssetWeights]
        STR         _Weights            // Stack: []

        // Load Index Order
        LDV         order_id            // Stack: [Order = (Collateral, Spent, Minted)] 
        UNPK                            // Stack: [Collateral, Spent, Minted]
        STR         _Minted             // Stack: [Collateral, Spent]
        STR         _Spent              // Stack: [Collateral]
        STR         _Collateral         // Stack: []

        // Compute Index Quantity
        LDV         quote_id            // Stack: [Quote = (Capacity, Price, Slope)]
        UNPK                            // Stack: [Capacity, Price, Slope]
        SWAP        2                   // Stack: [Slope, Price, Capacity]
        STR         _Capacity           // Stack: [Slope, Price]
        LDM         _Collateral         // Stack: [Slope, Price, Collateral]

        B  solve_quadratic_id  3  1  4  // Stack: [IndexQuantity]
        STR         _IndexQuantity      // Stack: []

        // Cap Index Quantity with Capacity
        LDM         _Capacity               // Stack: [Capacity]
        LDR         _IndexQuantity          // Stack: [Capacity, IndexQuantity]
        MIN         1                       // Stack: [Capacity, CappedIndexQuantity]
        STR         _CappedIndexQuantity    // Stack: [Capacity]
        POPN        1                       // Stack: []

        // Generate Individual Asset Orders (compute asset quantities)
        LDR         _CappedIndexQuantity    // Stack: [CappedIndexQuantity]
        LDM         _Weights                // Stack: [CappedIndexQuantity, AssetWeights]
        MUL         1                       // Stack: [CappedIndexQuantity]
        
        // TODO: Remove this, we don't want to store those on-chain
        STV         order_quantities_id // Stack: []

        // TODO: 
        // - match Inventory Demand
        // - update Delta

        LDR         _CappedIndexQuantity    // Stack: [CappedIndexQuantity] 
        LDR         _IndexQuantity          // Stack: [CappedIndexQuantity, IndexQuantity]
        SUB         1                       // Stack: [CappedIndexQuantity, IndexQuantityRemaining]
        STR         _IndexQuantityRemaining // Stack: [CappedIndexQuantity]
        POPN        1                       // Stack: []

        // TODO:
        // - push remaining quantity for later (?)
        // - capped index quantity is the we amount executed (need to pass it back for minting)
    }
}
