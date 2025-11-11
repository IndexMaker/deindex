use devil_macros::devil;

/// Solve Index Quantity Equation: (S, P, C) -> Q = C / (P + S * Q)
/// 
pub fn solve_quadratic() -> Vec<u128> {
    devil! {
        // 1. Initial Load and Setup (assuming stack starts with [C_vec, P_vec, S_vec])
        STR     _C           // C_vec -> R3, POP C_vec
        STR     _P           // P_vec -> R2, POP P_vec
        STR     _S           // S_vec -> R1, POP S_vec

        // 2. Compute P^2 (R4)
        LDR     _P
        MUL     0           // P^2 = P * P (Vector self-multiplication)
        STR     _P2         // P^2 -> R4, POP P^2

        // 3. Compute Radical (R5)
        LDR     _S
        LDM     _C
        MUL     1           // [S, SC] (Vector * Vector)
        IMMS    4
        MUL     1           // [S, SC, 4SC] (Vector * Scalar)
        LDM     _P2         // [S, SC, 4SC, P^2]
        ADD     1           // [S, SC, 4SC, P^2+4SC] (Vector + Vector)
        SQRT                // [S, SC, 4SC, R] (Vector square root)

        // 4. Compute Numerator: N = max(R - P, 0)
        LDM     _P          // [..., R, P]
        SWAP    1           // [..., P, R]
        SSB     1           // [..., P, N] (Vector - Vector subtraction)

        // 5. Compute X = Num / 2S
        LDM     _S
        IMMS    2           // [..., min, N, S, 2]
        SWAP    1           // [..., min, N, 2, S]
        MUL     1           // [..., min, N, 2, 2S] (Vector * Scalar multiplication)

        SWAP    2           // [..., min, 2S, 2, N] (N at pos 0, 2S at pos 2)
        DIV     2           // [..., min, 2S, 2, X]. X = N / 2S. (Vector / Vector division)
        // Final Vector X is at the top of the stack.
    }
}
