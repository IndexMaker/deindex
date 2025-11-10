#[macro_export]
macro_rules! devil {
    // --- Rule: End of Input ---
    (@accumulate [$( $body:tt )*] @line_start) => {
        vec![ $( $body )* ]
    };

    // --- Rule: Handle Comment Line (;) ---
    (@accumulate [$( $body:tt )*] @line_start ; $($rest:tt)*) => {
        devil! { @accumulate [$( $body )*] @line_start $($rest)* }
    };
    
    // --- Rule: Handle `IMMS <float>` (Special Case: Argument is Amount::to_u128_raw()) ---
    (@accumulate [$( $body:tt )*] @line_start IMMS $val:literal $($rest:tt)*) => {
        devil! {
            @accumulate
            [$( $body )* OP_IMMS, amount_macros::amount!($val).to_u128_raw(),]
            $($rest)*
        }
    };

    // --- Rule: Handle Multiple Arguments (N=4, B/FOLD) ---
    // Matches: B 1 2 3 4
    (@accumulate [$( $body:tt )*] @line_start $op:ident $a:literal $b:literal $c:literal $d:literal $($rest:tt)*) => {
        devil! {
            @lookup $op $a $b $c $d => 
            @accumulate [$( $body )*] 
            $($rest)*
        }
    };

    // --- Rule: Handle Double Arguments (N=2, JADD/JSSB/etc.) ---
    // Matches: JADD 1 2
    (@accumulate [$( $body:tt )*] @line_start $op:ident $a:literal $b:literal $($rest:tt)*) => {
        devil! {
            @lookup $op $a $b => 
            @accumulate [$( $body )*] 
            $($rest)*
        }
    };

    // --- Rule: Handle Single Argument (N=1, STR/LDR/ADD/etc.) ---
    // Matches: STR 1
    (@accumulate [$( $body:tt )*] @line_start $op:ident $num:literal $($rest:tt)*) => {
        devil! {
            @lookup $op $num =>
            @accumulate [$( $body )*]
            $($rest)*
        }
    };

    // --- Rule: Handle No Arguments (N=0, SQRT/UNPK/etc.) ---
    // Matches: SQRT
    (@accumulate [$( $body:tt )*] @line_start $op:ident $($rest:tt)*) => {
        devil! {
            @lookup $op => 
            @accumulate [$( $body )*] 
            $($rest)*
        }
    };

    // --- Rule: Main Accumulator (General recursion) ---
    (@accumulate [$( $body:tt )*] $($input:tt)*) => {
        devil! { @accumulate [$( $body )*] @line_start $($input)* }
    };

    // --- Mnemonic Lookup Rules: 4 Arguments (B, FOLD) ---
    (@lookup B $a:literal $b:literal $c:literal $d:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => {
        devil! { @accumulate [$( $body )* OP_B, $a, $b, $c, $d,] @line_start $($rest)* }
    };
    (@lookup FOLD $a:literal $b:literal $c:literal $d:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => {
        devil! { @accumulate [$( $body )* OP_FOLD, $a, $b, $c, $d,] @line_start $($rest)* }
    };

    // --- Mnemonic Lookup Rules: 2 Arguments (JADD, JSSB, JXPND, JFLTR) ---
    (@lookup JADD $a:literal $b:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => {
        devil! { @accumulate [$( $body )* OP_JADD, $a, $b,] @line_start $($rest)* }
    };
    (@lookup JSSB $a:literal $b:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => {
        devil! { @accumulate [$( $body )* OP_JSSB, $a, $b,] @line_start $($rest)* }
    };
    (@lookup JXPND $a:literal $b:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => {
        devil! { @accumulate [$( $body )* OP_JXPND, $a, $b,] @line_start $($rest)* }
    };
    (@lookup JFLTR $a:literal $b:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => {
        devil! { @accumulate [$( $body )* OP_JFLTR, $a, $b,] @line_start $($rest)* }
    };

    // --- Mnemonic Lookup Rules: 1 Argument (LDL, LDV, ADD, STR, etc.) ---
    // (This block is extensive; showing a few examples)
    (@lookup LDL $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LDL, $num,] @line_start $($rest)* } };
    (@lookup LDV $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LDV, $num,] @line_start $($rest)* } };
    (@lookup LDS $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LDS, $num,] @line_start $($rest)* } };
    (@lookup LDD $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LDD, $num,] @line_start $($rest)* } };
    (@lookup LDR $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LDR, $num,] @line_start $($rest)* } };
    (@lookup STL $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_STL, $num,] @line_start $($rest)* } };
    (@lookup STV $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_STV, $num,] @line_start $($rest)* } };
    (@lookup STS $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_STS, $num,] @line_start $($rest)* } };
    (@lookup STR $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_STR, $num,] @line_start $($rest)* } };
    (@lookup PKV $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_PKV, $num,] @line_start $($rest)* } };
    (@lookup PKL $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_PKL, $num,] @line_start $($rest)* } };
    (@lookup VPUSH $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_VPUSH, $num,] @line_start $($rest)* } };
    (@lookup T $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_T, $num,] @line_start $($rest)* } };
    (@lookup LUNION $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LUNION, $num,] @line_start $($rest)* } };
    (@lookup LPUSH $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LPUSH, $num,] @line_start $($rest)* } };
    (@lookup ADD $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_ADD, $num,] @line_start $($rest)* } };
    (@lookup SUB $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_SUB, $num,] @line_start $($rest)* } };
    (@lookup SSB $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_SSB, $num,] @line_start $($rest)* } };
    (@lookup MUL $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_MUL, $num,] @line_start $($rest)* } };
    (@lookup DIV $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_DIV, $num,] @line_start $($rest)* } };
    (@lookup MIN $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_MIN, $num,] @line_start $($rest)* } };
    (@lookup MAX $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_MAX, $num,] @line_start $($rest)* } };
    (@lookup ZEROS $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_ZEROS, $num,] @line_start $($rest)* } };
    (@lookup ONES $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_ONES, $num,] @line_start $($rest)* } };
    (@lookup POPN $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_POPN, $num,] @line_start $($rest)* } };
    (@lookup SWAP $num:literal => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_SWAP, $num,] @line_start $($rest)* } };

    // --- Mnemonic Lookup Rules: 0 Arguments (SQRT, UNPK, VSUM, etc.) ---
    (@lookup UNPK => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_UNPK,] @line_start $($rest)* } };
    (@lookup VPOP => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_VPOP,] @line_start $($rest)* } };
    (@lookup LPOP => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_LPOP,] @line_start $($rest)* } };
    (@lookup SQRT => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_SQRT,] @line_start $($rest)* } };
    (@lookup VSUM => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_VSUM,] @line_start $($rest)* } };
    (@lookup VMIN => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_VMIN,] @line_start $($rest)* } };
    (@lookup VMAX => @accumulate [$( $body:tt )*] $($rest:tt)*) => { devil! { @accumulate [$( $body )* OP_VMAX,] @line_start $($rest)* } };
    
    // --- Initial Entry Point ---
    ($($input:tt)*) => {
        devil! { @accumulate [] @line_start $($input)* }
    };
}