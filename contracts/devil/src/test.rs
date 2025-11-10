use std::collections::HashMap;

use amount_macros::amount;
use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector, vis::*};
use devil_macros::devil;
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

    let solve_quadratic_vectorized = devil! {
        // 1. Initial Load and Setup (assuming stack starts with [C_vec, P_vec, S_vec])
        STR     _S           // S_vec -> R1, POP S_vec
        STR     _P           // P_vec -> R2, POP P_vec
        STR     _C           // C_vec -> R3, POP C_vec

        // 2. Compute P^2 (R4)
        LDR     _P
        MUL     0           // P^2 = P * P (Vector self-multiplication)
        STR     _P2         // P^2 -> R4, POP P^2

        // 3. Compute Radical (R5)
        LDR     _S
        LDR     _C
        MUL     1           // [S, SC] (Vector * Vector)
        IMMS    4
        MUL     1           // [S, SC, 4SC] (Vector * Scalar)
        LDR     _P2         // [S, SC, 4SC, P^2]
        ADD     1           // [S, SC, 4SC, P^2+4SC] (Vector + Vector)
        SQRT                // [S, SC, 4SC, R] (Vector square root)

        // 4. Compute Numerator: N = max(R - P, 0)
        LDR     _P          // [..., R, P]
        SWAP    1           // [..., P, R]
        SSB     1           // [..., P, N] (Vector - Vector subtraction)

        // 5. Compute X = Num / 2S
        LDR     _S
        IMMS    2           // [..., min, N, S, 2]
        SWAP    1           // [..., min, N, 2, S]
        MUL     1           // [..., min, N, 2, 2S] (Vector * Scalar multiplication)

        SWAP    2           // [..., min, 2S, 2, N] (N at pos 0, 2S at pos 2)
        DIV     2           // [..., min, 2S, 2, X]. X = N / 2S. (Vector / Vector division)
        // Final Vector X is at the top of the stack.
    };

    vio.store_labels(
        solve_quadratic_id,
        Labels {
            data: solve_quadratic_vectorized,
        },
    )
    .unwrap();

    let code = devil! {
        LDV         weights_id          // Stack: [AW]
        STR         _AW                 // Stack: []

        // Extract Collateral (Order vector: [Collateral, Spent, Minted])
        LDV         order_id            // Stack: [O]
        UNPK                            // Stack: [Minted, Spent, Collateral]
        POPN        2                   // Stack: [Collateral]
        STR         _C                  // Stack: []

        // Extract Price and Slope (Quote vector: [Capacity, Price, Slope])
        LDV         quote_id            // Stack: [Q]
        UNPK                            // Stack: [Slope, Price, Capacity]
        POPN        1                   // Stack: [Slope, Price] (Capacity discarded)

        // Stack is now [Slope, Price]. Load Collateral to get arguments in order.
        LDR         _C                  // Stack: [Slope, Price, Collateral]

        // Call procedure: Inputs are Collateral (TOS), Price, Slope.
        B  solve_quadratic_id  3  1  4  // Stack: [IndexQuantity]

        // Apply Weights and Store Result
        LDR         _AW                 // Stack: [IQ, AW]
        MUL         1                   // Stack: [AssetQuantities]
        STV         order_quantities_id // Stack: []
    };

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
        amount_vec![
            0.000099990001950000,
            0.000999900019500000,
            0.099990001950000000
        ]
        .data
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
