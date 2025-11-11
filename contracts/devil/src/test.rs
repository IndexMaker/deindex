use std::collections::HashMap;

use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector, vis::*};
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
    scalars: HashMap<u128, Amount>,
}

impl TestVectorIO {
    fn new() -> Self {
        Self {
            labels: HashMap::new(),
            vectors: HashMap::new(),
            scalars: HashMap::new(),
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

    fn load_scalar(&self, id: u128) -> Result<Amount, ErrorCode> {
        let v = self.scalars.get(&id).ok_or_else(|| ErrorCode::NotFound)?;
        Ok(*v)
    }

    fn store_labels(&mut self, id: u128, input: Labels) -> Result<(), ErrorCode> {
        self.labels.insert(id, input);
        Ok(())
    }

    fn store_vector(&mut self, id: u128, input: Vector) -> Result<(), ErrorCode> {
        self.vectors.insert(id, input);
        Ok(())
    }

    fn store_scalar(&mut self, id: u128, input: Amount) -> Result<(), ErrorCode> {
        self.scalars.insert(id, input);
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
    let assets_id = 101;
    let weights_id = 102;
    let quote_id = 201;
    let order_id = 301;
    let order_quantities_id = 401;
    let solve_quadratic_id = 901;

    vio.store_labels(assets_id, label_vec![1001, 1002, 1003])
        .unwrap();

    vio.store_vector(weights_id, amount_vec![0.100, 1.000, 100.0])
        .unwrap();

    vio.store_vector(quote_id, amount_vec![10.00, 10_000, 100.0])
        .unwrap();

    vio.store_vector(order_id, amount_vec![1000.00, 0, 0])
        .unwrap();

    vio.store_labels(
        solve_quadratic_id,
        Labels {
            data: solve_quadratic(),
        },
    )
    .unwrap();

    let code = execute_buy_order(
        order_id,
        weights_id,
        quote_id,
        solve_quadratic_id,
        order_quantities_id,
    );
    // P = 10 000
    // S = 100
    // C = 1000

    let num_registers = 8;

    let mut program = Program::new(&mut vio);
    let mut stack = Stack::new(num_registers);
    let result = program.execute_with_stack(code, &mut stack);

    if let Err(err) = result {
        log_stack!(&stack);
        panic!("Failed to execute test: {:?}", err);
    }

    let order = vio.load_vector(order_id).unwrap();
    let quote = vio.load_vector(quote_id).unwrap();
    let weigths = vio.load_vector(weights_id).unwrap();
    let order_quantites = vio.load_vector(order_quantities_id).unwrap();

    log_msg!("\n-= Program complete =-");
    log_msg!("[in] Order = {:0.9}", order);
    log_msg!("[in] Quote = {:0.9}", quote);
    log_msg!("[in] Weights = {:0.9}", weigths);
    log_msg!("[out] Order Quantities = {:0.9}", order_quantites);

    // [in] Order = 1000.000000000,0.000000000,0.000000000
    // [in] Quote = 10.000000000,10000.000000000,100.000000000
    // [in] Weights = 0.100000000,1.000000000,100.000000000
    // [out] Order Quantities = 0.000099990,0.000999900,0.099990001

    assert_eq!(order.data, amount_vec![1000, 0, 0].data);
    assert_eq!(quote.data, amount_vec![10, 10_000, 100].data);
    assert_eq!(weigths.data, amount_vec![0.1, 1, 100,].data);

    // these are exact expected fixed point decimal values as raw u128
    assert_eq!(
        order_quantites.data,
        amount_vec![0.00999001995, 0.0999001995, 9.990019950].data
    );
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
