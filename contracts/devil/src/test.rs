use std::collections::HashMap;

use deli::{labels::Labels, log_msg, vector::Vector};
use devil_macros::devil;
use icore::vil::execute_buy_order::execute_buy_order;
use icore::vil::solve_quadratic::solve_quadratic;
use labels_macros::label_vec;
use vector_macros::amount_vec;

use crate::log_stack;
use crate::program::*; // Use glob import for tidiness

struct TestVectorIO {
    labels: HashMap<u128, Labels>,
    vectors: HashMap<u128, Vector>,
}

impl TestVectorIO {
    fn new() -> Self {
        Self {
            labels: HashMap::new(),
            vectors: HashMap::new(),
        }
    }
}

impl VectorIO for TestVectorIO {
    fn load_labels(&self, id: u128) -> Result<Labels, ErrorCode> {
        let v = self.labels.get(&id).ok_or_else(|| ErrorCode::NotFound)?;
        Ok(Labels {
            data: v.data.clone(),
        })
    }

    fn load_vector(&self, id: u128) -> Result<Vector, ErrorCode> {
        let v = self.vectors.get(&id).ok_or_else(|| ErrorCode::NotFound)?;
        Ok(Vector {
            data: v.data.clone(),
        })
    }

    fn store_labels(&mut self, id: u128, input: Labels) -> Result<(), ErrorCode> {
        self.labels.insert(id, input);
        Ok(())
    }

    fn store_vector(&mut self, id: u128, input: Vector) -> Result<(), ErrorCode> {
        self.vectors.insert(id, input);
        Ok(())
    }
}

/// All round test verifies that majority of VIL functionality works as expected.
///
/// We test:
/// - load and store of vectors (externally via VectorIO)
/// - load and store of values into registry
/// - invocation of sub-routines w/ parameters and return values
/// - example implementation of Solve-Quadratic function (vectorised)
/// - example implementation of index order asset quantity computation from index asset weights
///
/// The purpose of VIL is to allow generic vector operations in Stylus smart-contracts, so that:
/// - vector data is loaded and stored into blockchain once and only once
/// - vector data is modified in-place whenever possible, and only duplicated when necessary
/// - labels and join operations allow sparse vector addition and saturating subtraction
/// - data and operations live in the same smart-contract without exceeding 24KiB WASM limit
/// - other smart-contracts can submit VIL code to execute without them-selves exceeding 24KiB WASM limit
/// - minimisation of gas use by reduction of blockchain operations and executed instructions
///
/// NOTE: while VIL is an assembly language, it is limitted exclusively to perform vector math, and
/// instruction set is designed to particularly match our requirements to execute index orders and
/// update inventory.
///
/// TBD: examine real-life gas usage and limits.
///
#[test]
fn test_compute_1() {
    let mut vio = TestVectorIO::new();
    let index_order_id = 10001;
    let executed_asset_quantities_id = 10002;
    let executed_index_quantities_id = 10003;
    let asset_names_id = 1001;
    let weights_id = 1002;
    let quote_id = 1003;
    let inventory_asset_names_id = 101;
    let supply_long_id = 102;
    let supply_short_id = 103;
    let demand_long_id = 104;
    let demand_short_id = 105;
    let delta_long_id = 106;
    let delta_short_id = 107;
    let solve_quadratic_id = 10;

    vio.store_labels(asset_names_id, label_vec![51, 53, 54])
        .unwrap();

    vio.store_vector(weights_id, amount_vec![0.100, 1.000, 100.0])
        .unwrap();

    vio.store_vector(quote_id, amount_vec![10.00, 10_000, 100.0])
        .unwrap();

    vio.store_vector(index_order_id, amount_vec![1000.00, 0, 0])
        .unwrap();

    vio.store_labels(inventory_asset_names_id, label_vec![51, 52, 53, 54, 55])
        .unwrap();

    vio.store_vector(demand_short_id, amount_vec![0, 0, 0.01, 0, 0])
        .unwrap();

    vio.store_vector(demand_long_id, amount_vec![0.1, 0.1, 0, 0.01, 0.2])
        .unwrap();

    vio.store_vector(supply_short_id, amount_vec![0, 0, 0, 0, 0])
        .unwrap();

    vio.store_vector(supply_long_id, amount_vec![0.05, 0.05, 0.05, 0.05, 0.05])
        .unwrap();

    vio.store_vector(delta_short_id, amount_vec![0, 0, 0, 0, 0])
        .unwrap();

    vio.store_vector(delta_long_id, amount_vec![0, 0, 0, 0, 0])
        .unwrap();

    vio.store_labels(
        solve_quadratic_id,
        Labels {
            data: solve_quadratic(),
        },
    )
    .unwrap();

    let code = execute_buy_order(
        index_order_id,
        executed_index_quantities_id,
        executed_asset_quantities_id,
        asset_names_id,
        weights_id,
        quote_id,
        inventory_asset_names_id,
        supply_long_id,
        supply_short_id,
        demand_long_id,
        demand_short_id,
        delta_long_id,
        delta_short_id,
        solve_quadratic_id,
    );

    let num_registers = 16;

    let mut program = Program::new(&mut vio);
    let mut stack = Stack::new(num_registers);
    let result = program.execute_with_stack(code, &mut stack);

    if let Err(err) = result {
        log_stack!(&stack);
        panic!("Failed to execute test: {:?}", err);
    }

    let order = vio.load_vector(index_order_id).unwrap();
    let quote = vio.load_vector(quote_id).unwrap();
    let weigths = vio.load_vector(weights_id).unwrap();
    let index_quantites = vio.load_vector(executed_index_quantities_id).unwrap();
    let asset_quantites = vio.load_vector(executed_asset_quantities_id).unwrap();
    let demand_short = vio.load_vector(demand_short_id).unwrap();
    let demand_long = vio.load_vector(demand_long_id).unwrap();
    let delta_short = vio.load_vector(delta_short_id).unwrap();
    let delta_long = vio.load_vector(delta_long_id).unwrap();

    log_msg!("\n-= Program complete =-");
    log_msg!("\n[in] Index Order = {:0.9}", order);
    log_msg!("[in] Index Quote = {:0.9}", quote);
    log_msg!("[in] Asset Weights = {:0.9}", weigths);
    log_msg!("\n[out] Index Quantities = {:0.9}", index_quantites);
    log_msg!("[out] Asset Quantities = {:0.9}", asset_quantites);
    log_msg!("\n[out] Demand Short = {:0.9}", demand_short);
    log_msg!("[out] Demand Long = {:0.9}", demand_long);
    log_msg!("\n[out] Delta Short = {:0.9}", delta_short);
    log_msg!("[out] Delta Long = {:0.9}", delta_long);

    assert_eq!(order.data, amount_vec![1000, 0, 0].data);
    assert_eq!(quote.data, amount_vec![10, 10_000, 100].data);
    assert_eq!(weigths.data, amount_vec![0.1, 1, 100,].data);

    // these are exact expected fixed point decimal values as raw u128
    assert_eq!(
        index_quantites.data,
        amount_vec![0.0999001995, 0.000000000].data
    );
    assert_eq!(
        asset_quantites.data,
        amount_vec![0.00999001995, 0.0999001995, 9.99001995].data
    );
    assert_eq!(demand_short.data, amount_vec![0, 0, 0, 0, 0].data);
    assert_eq!(
        demand_long.data,
        amount_vec![0.10999001995, 0.1, 0.0899001995, 10.00001995, 0.2].data
    );
    assert_eq!(
        delta_short.data,
        amount_vec![0.05999001995, 0.05, 0.0399001995, 9.95001995, 0.15].data
    );
    assert_eq!(delta_long.data, amount_vec![0, 0, 0, 0, 0].data);
}

#[test]
fn test_transpose() {
    let mut vio = TestVectorIO::new();
    let num_registers = 8;

    // --- 1. Setup VIO Inputs ---
    let vector1_id = 100;
    let vector2_id = 101;
    let expected1_id = 102; // T1: [1, 4]
    let expected2_id = 103; // T2: [2, 5]
    let expected3_id = 104; // T3: [3, 6]
    let delta_id = 105;

    vio.store_vector(vector1_id, amount_vec![1, 2, 3]).unwrap();
    vio.store_vector(vector2_id, amount_vec![4, 5, 6]).unwrap();
    vio.store_vector(expected1_id, amount_vec![1, 4]).unwrap();
    vio.store_vector(expected2_id, amount_vec![2, 5]).unwrap();
    vio.store_vector(expected3_id, amount_vec![3, 6]).unwrap();

    // --- 2. VIL Code Execution ---
    let code = devil![
        // 1. Setup Transposition
        LDV         vector1_id              // Stack: [V1]
        LDV         vector2_id              // Stack: [V1, V2]
        T           2                       // Stack: [T1, T2, T3] (3 vectors)

        // 2. Load Expected Vectors for comparison
        LDV         expected1_id            // [T1, T2, T3, E1]
        LDV         expected2_id            // [T1, T2, T3, E1, E2]
        LDV         expected3_id            // [T1, T2, T3, E1, E2, E3] (6 vectors)

        // 3. D3 = T3 - E3
        SUB         3                       // Stack: [T1, T2, T3, E1, E2, D3]

        // 4. D2 = T2 - E2
        SWAP        1                       // Stack: [T1, T2, T3, E1, D3, E2]
        SUB         4                       // Stack: [T1, T2, T3, E1, D3, D2]

        // 5. D1 = T1 - E1
        SWAP        2                       // Stack: [T1, T2, T3, D2, D3, E1]
        SUB         5                       // Stack: [T1, T2, T3, D2, D3, D1]

        // 6. Compute total delta - should be zero
        ADD         1                       // Stack: [T1, T2, T3, D2, D3, D1 + D3]
        ADD         2                       // Stack: [T1, T2, T3, D2, D3, D1 + D3 + D2]

        // 7. Store the final zero vector
        STV         delta_id
    ];

    let mut stack = Stack::new(num_registers);
    let mut program = Program::new(&mut vio);

    if let Err(err) = program.execute_with_stack(code, &mut stack) {
        log_stack!(&stack);
        panic!("Failed to execute test: {:?}", err);
    }

    // --- 3. Assertion ---
    let delta = vio.load_vector(delta_id).unwrap();

    assert_eq!(delta.data, amount_vec![0, 0].data);
}
