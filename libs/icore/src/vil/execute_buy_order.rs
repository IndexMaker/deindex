use devil_macros::devil;

/// Execute Buy Index Order
/// 
pub fn execute_buy_order(
    order_id: u128,
    executed_index_quantities_id: u128,
    executed_asset_quantities_id: u128,
    asset_names_id: u128,
    asset_weights_id: u128,
    index_quote_id: u128,
    inventory_asset_names_id: u128,
    supply_long_id: u128,
    supply_short_id: u128,
    demand_long_id: u128,
    demand_short_id: u128,
    delta_long_id: u128,
    delta_short_id: u128,
    solve_quadratic_id: u128,
) -> Vec<u128> {
    devil! {
        // Load Weights
        LDV         asset_weights_id            // Stack: [AssetWeights]
        STR         _Weights                    // Stack: []

        // Load Index Order
        LDV         order_id                    // Stack: [Order = (Collateral, Spent, Minted)] 
        UNPK                                    // Stack: [Collateral, Spent, Minted]
        STR         _Minted                     // Stack: [Collateral, Spent]
        STR         _Spent                      // Stack: [Collateral]
        STR         _Collateral                 // Stack: []

        // Compute Index Quantity
        LDV         index_quote_id                    // Stack: [Quote = (Capacity, Price, Slope)]
        UNPK                                    // Stack: [Capacity, Price, Slope]
        SWAP        2                           // Stack: [Slope, Price, Capacity]
        STR         _Capacity                   // Stack: [Slope, Price]
        LDM         _Collateral                 // Stack: [Slope, Price, Collateral]

        B  solve_quadratic_id  3  1  4          // Stack: [IndexQuantity]
        STR         _IndexQuantity              // Stack: []

        // Cap Index Quantity with Capacity
        LDM         _Capacity                   // Stack: [Capacity]
        LDR         _IndexQuantity              // Stack: [Capacity, IndexQuantity]
        MIN         1                           // Stack: [Capacity, CIQ = MIN(Capacity, IndexQuantity)]
        STR         _CappedIndexQuantity        // Stack: [Capacity]
        POPN        1                           // Stack: []

        // Generate Individual Asset Orders (compute asset quantities)
        LDR         _CappedIndexQuantity        // Stack: [CIQ]
        LDM         _Weights                    // Stack: [CIQ, AssetWeights]
        MUL         1                           // Stack: [CIQ, AssetQuantities]
        
        STR         _AssetQuantities            // Stack: [CIQ]
        POPN        1                           // Stack: []

        // TODO: 
        // - match Inventory Demand
        LDL         asset_names_id              // Stack [AssetNames]
        LDL         inventory_asset_names_id    // Stack [AssetNames, InventoryAssetNames]
        
        // Compute Demand Short = MAX(Demand Short - Asset Quantities, 0)
        LDV         demand_short_id             // Stack [AssetNames, InventoryAssetNames, DS_old]
        LDR         _AssetQuantities            // Stack [AssetNames, InventoryAssetNames, DS_old, AQ]
        LDD         1                           // Stack [AssetNames, InventoryAssetNames, DS_old, AQ, DS_old]
        JFLT        3   4                       // Stack [AssetNames, InventoryAssetNames, DS_old, AQ, fDS_old]
        LDD         0                           // Stack [AssetNames, InventoryAssetNames, DS_old, AQ, fDS_old, fDS_old]
        SSB         2                           // Stack [AssetNames, InventoryAssetNames, DS_old, AQ, fDS_old, fDS_new = (fDS_old s- AQ)]
        SWAP        3                           // Stack [AssetNames, InventoryAssetNames, fDS_new, AQ, fDS_old, DS_old]
        JUPD        3   4   5                   // Stack [AssetNames, InventoryAssetNames, fDS_new, AQ, fDS_old, DS_new]
        SWAP        3                           // Stack [AssetNames, InventoryAssetNames, DS_new, AQ, fDS_old, fDS_new]
        POPN        1                           // Stack [AssetNames, InventoryAssetNames, DS_new, AQ, fDS_old]

        // Compute Demand Long += MAX(Asset Quantities - Demand Short, 0)
        SWAP        1                           // Stack [AssetNames, InventoryAssetNames, DS_new, fDS_old, AQ]
        SSB         1                           // Stack [AssetNames, InventoryAssetNames, DS_new, fDS_old, dAQ = (AQ s- fDS_old)]
        LDV         demand_long_id              // Stack [AssetNames, InventoryAssetNames, DS_new, fDS_old, dAQ, DL_old]
        JADD        1   4   5                   // Stack [AssetNames, InventoryAssetNames, DS_new, fDS_old, dAQ, DL_new = (DL_old j+ dAQ)]
        SWAP        2                           // Stack [AssetNames, InventoryAssetNames, DS_new, DL_new, dAQ, fDS_old]
        POPN        2                           // Stack [AssetNames, InventoryAssetNames, DS_new, DL_new]
        STR         _DemandLong                 // Stack [AssetNames, InventoryAssetNames, DS_new]
        STR         _DemandShort                // Stack [AssetNames, InventoryAssetNames]
        
        // Update Delta
        //
        // (Delta Long - Delta Short) = (Supply Long + Demand Short) - (Supply Short + Demand Long)
        //
        
        // Supply Long + Demand Short
        LDV         supply_long_id
        LDR         _DemandShort
        ADD         1                           // Stack [AssetNames, InventoryAssetNames, SupplyLong, DeltaLong]
        SWAP        1
        POPN        1                           // Stack [AssetNames, InventoryAssetNames, DeltaLong]

        // Supply Short + Demand Long
        LDV         supply_short_id
        LDR         _DemandLong
        ADD         1                           // Stack [AssetNames, InventoryAssetNames, DeltaLong, SupplyShort, DeltaShort]
        SWAP        1
        POPN        1                           // Stack [AssetNames, InventoryAssetNames, DeltaLong, DeltaShort]

        // Delta Long - Delta Short
        LDD         0                           // Stack [AssetNames, InventoryAssetNames, DeltaLong, DeltaShort, DeltaShort]
        SSB         2                           // Stack [AssetNames, InventoryAssetNames, DeltaLong, DeltaShort, RS = (DeltaShort s- DeltaLong)]
        STR         _DeltaShort                 // Stack [AssetNames, InventoryAssetNames, DeltaLong, DeltaShort]
        SWAP        1                           // Stack [AssetNames, InventoryAssetNames, DeltaShort, DeltaLong]
        SSB         1                           // Stack [AssetNames, InventoryAssetNames, DeltaShort, RL = (DeltaLong s- DeltaShort)]
        STR         _DeltaLong                  // Stack [AssetNames, InventoryAssetNames, DeltaShort]
        POPN        3                           // Stack []

        // Store Demand
        LDM         _DemandLong
        LDM         _DemandShort
        STV         demand_short_id
        STV         demand_long_id

        // Store Delta
        LDM         _DeltaLong
        LDM         _DeltaShort
        STV         delta_short_id
        STV         delta_long_id

        // Store Executed Index Quantity and Remaining Quantity
        LDM         _CappedIndexQuantity            // Stack: [CIQ] 
        LDM         _IndexQuantity                  // Stack: [CIQ, IndexQuantity]
        SUB         1                               // Stack: [CIQ, RIQ = (IndexQuantity - CIQ)]
        PKV         2                               // Stack: [(CIQ, RIQ)]
        STV         executed_index_quantities_id    // Stack: []
        
        // Store Executed Asset Quantities
        LDM         _AssetQuantities
        STV         executed_asset_quantities_id
    }
}
