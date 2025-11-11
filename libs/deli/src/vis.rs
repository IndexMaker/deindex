// Vector Instruction Set (VIS) for Vector IL (VIL) Virtual Machine

// 1. Data Loading & Stack Access (10-14)
pub const OP_LDL: u128 = 10; //   LDL <label_id>                ; no stack args ; result = [TOS: Labels]; Load Labels object from VIO by ID. Pushes on TOS.
pub const OP_LDV: u128 = 11; //   LDV <vector_id>               ; no stack args ; result = [TOS: Vector]; Load Vector object from VIO by ID. Pushes on TOS.
pub const OP_LDS: u128 = 12; //   LDS <scalar_id>               ; no stack args ; result = [TOS: Scalar]; Load Scalar value from VIO by ID. Pushes on TOS.

pub const OP_LDD: u128 = 13; //   LDD <pos>                     ; stack args = [TOS - pos] ; result = [TOS]; Load Duplicate (copy) of stack operand at [T-pos]. Pushes on TOS.
pub const OP_LDR: u128 = 14; //   LDR <reg>                     ; no stack args ; result = [TOS - pos] ; Load value from Registry (R0-Rn). Pushes on TOS.
pub const OP_LDM: u128 = 15; //   LDM <reg>                     ; no stack args ; result = [TOS - pos] ; Load value moving it out of Registry (R0-Rn). Value is removed from registry. Pushes on TOS.

// 2. Data Storage & Register Access (20-23)
pub const OP_STL: u128 = 20; //   STL <label_id>                ; stack args = [TOS: Labels] ; Store Labels object into VIO. Consumes TOS.
pub const OP_STV: u128 = 21; //   STV <vector_id>               ; stack args = [TOS: Vector] ; Store Vector object into VIO. Consumes TOS.
pub const OP_STS: u128 = 22; //   STS <scalar_id>               ; stack args = [TOS: Scalar] ; Store Scalar value into VIO. Consumes TOS.
pub const OP_STR: u128 = 23; //   STR <reg>                     ; stack args = [TOS - pos] ; stack unchanged, result in registry[reg]; Store into Registry (R0-Rn). Consumes TOS.

// 3. Data Structure Manipulation (30-35)
pub const OP_PKV: u128 = 30; //   PKV <count>                   ; stack args = [TOS - count, ..., TOS: Scalar] ; result [TOS]; Pack `count` values from stack into a new Vector. Consumes `count` operands from TOS, and replaces them with Vector.
pub const OP_PKL: u128 = 31; //   PKL <count>                   ; stack args = [TOS - count, ..., TOS: Scalar] ; result [TOS]; Pack `count` values from stack into a new Labels object. Consumes `count` operands from TOS, and replaces them with Labels.
pub const OP_UNPK: u128 = 32; //  UNPK                          ; stack args = [TOS: Vector|Labels]; result [TOS - len, ..., TOS] ; Unpack a Vector/Labels object onto the stack. Consumes TOS, and replaces with its components.
pub const OP_VPUSH: u128 = 33; // VPUSH <immediate (scalar)>    ; stack args = [TOS: Vector[0..len]] ; result = [TOS: Vector[0..len + 1]] ; Push a scalar onto the Vector (TOS). In-place updates Vector on TOS, appending new component at the end.
pub const OP_VPOP: u128 = 34; //  VPOP                          ; stack args = [TOS: Vector[0..len]] ; result = [TOS: Vector[0..len - 1]] ; Pop a scalar from the Vector (TOS). In-place updates Vector on TOS, removing last component.
pub const OP_T: u128 = 35; //     T <count>                     ; stack args = [TOS - count, ..., TOS: Vector[0..len]] ; result = [TOS - len, ..., TOS: Vector[0..count]] ; Transpose `count` vectors on stack [V1, V2] -> [T1, T2]. In-place updates `count` operands from TOS by performing transform.

// 4. Labels Manipulation (40-46)
pub const OP_LUNION: u128 = 40; // LUNION <pos>                 ; stack args = [TOS - pos, TOS] ; result = [TOS] ; Union of two Labels operands (TOS and T-pos). Pushes on TOS.
pub const OP_LPUSH: u128 = 41; //  LPUSH <immediate (label)>    ; stack args = [TOS: Labels[0..len]] ; result = [TOS:Labels[0..len + 1]] ; Push a label value onto the Labels object (TOS). In-place updates Labels on TOS, appending new component at the end.
pub const OP_LPOP: u128 = 42; //   LPOP                         ; stack args = [TOS: Labels[0..len]] ; result = [TOS: Lables[0..len - 1]] ; Pop a label value from the Labels object (TOS). In-place updates Labels on TOS, removing last component.
pub const OP_JUPD: u128 = 43; //   JUPD <pos_B> <lab_A> <lab_B> ; stack args = [TOS - lab_A: 'LA, TOS - lab_B: Labels 'LB, TOS - pos_B, TOS: Vector: 'A] ; result = [TOS: 'A filtered mapped 'LB to 'LA]; Update using Labels. Expands vector at [TOS - pos_B] using labels at [TOS - lab_B] to match labels of TOS at [TOS - lab_A]. In-place updates TOS. Consumes TOS.
pub const OP_JADD: u128 = 44; //   JADD <pos_B> <lab_A> <lab_B> ; stack args = [TOS - lab_A: 'LA, TOS - lab_B: Labels 'LB, TOS - pos_B, TOS: Vector: 'A] ; result = [TOS: 'A expaned w/ 0 mapped 'LB to 'LA]; Add using Labels. Expands vector at [TOS - pos_B] using labels at [TOS - lab_B] to match labels of TOS at [TOS - lab_A]. In-place updates TOS. Consumes TOS.
pub const OP_JSSB: u128 = 45; //   JSSB <pos_B> <lab_A> <lab_B> ; stack args = [TOS - lab_A: 'LA, TOS - lab_B: Labels 'LB, TOS - pos_B, TOS: Vector: 'A] ; result = [TOS: 'A expaned w/ 0 mapped 'LB to 'LA]; Saturating sub using Labels. Expands vector at [TOS - pos_B] using labels at [TOS - lab_B] to match labels of TOS at [TOS - lab_A]. In-place updates TOS. Consumes TOS.
pub const OP_JXPN: u128 = 46; //   JXPN <lab_A> <lab_B>         ; stack args = [TOS - lab_A: 'LA, TOS - lab_B: Labels 'LB, TOS: Vector: 'A] ; result = [TOS: 'A expaned w/ 0 mapped 'LB to 'LA]; Expand using Labels. Expands vector at [TOS-1] using labels at [T-lab_B] to match labels of TOS at [T-lab_A]. In-place updates TOS. Does not consume other operands.
pub const OP_JFLT: u128 = 47; //   JFLT <lab_A> <lab_B>         ; stack args = [TOS - lab_A: 'LA, TOS - lab_B: Labels 'LB, TOS: Vector: 'A] ; result = [TOS: 'A filtered mapped 'LB to 'LA]; Filter using Labels. Expands vector at [TOS-1] using labels at [T-lab_B] to match labels of TOS at [T-lab_A]. In-place updates TOS. Does not consume other operands.

// 5. Arithmetic & Core Math (50-55)
pub const OP_ADD: u128 = 50; //    ADD <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS] ; Add TOS by operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.
pub const OP_SUB: u128 = 51; //    SUB <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS] ; Subtract TOS by operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.
pub const OP_SSB: u128 = 52; //    SSB <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS] ; Saturating subtract TOS by operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.
pub const OP_MUL: u128 = 53; //    MUL <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS] ; Multiply TOS by operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.
pub const OP_DIV: u128 = 54; //    DIV <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS] ; Divide TOS by operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.
pub const OP_SQRT: u128 = 55; //   SQRT                         ; stack args = [TOS: Vector|Scalar]; result = [TOS] ; Square root of TOS (scalar or component-wise vector). Works with vectors and scalars. In-place updates operand on TOS.

// 6. Logic & Comparison (60-61)
pub const OP_MIN: u128 = 60; //    MIN <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS: Vector|Scalar] ; Min between TOS and operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.
pub const OP_MAX: u128 = 61; //    MAX <pos>                    ; stack args = [TOS - pos, TOS: Vector|Scalar] ; result = [TOS: Vector|Scalar] ; Max between TOS and operand at [T-pos]. Works with vectors and scalars. In-place updates operand on TOS. Does not consume the other operand.

// 7. Vector Aggregation (70-72)
pub const OP_VSUM: u128 = 70; //   VSUM                         ; stack args = [TOS: Vector] ; result = [TOS: Scalar] ; Sum of all vector components. Pushes on TOS. Does not consume the operand.
pub const OP_VMIN: u128 = 71; //   VMIN                         ; stack args = [TOS: Vector] ; result = [TOS: Scalar] ; Minimum value found within vector components. Pushes on TOS. Does not consume the operand.
pub const OP_VMAX: u128 = 72; //   VMAX                         ; stack args = [TOS: Vector] ; result = [TOS: Scalar] ; Maximum value found within vector components. Pushes on TOS. Does not consume the operand.

// 8. Immediate Values & Vector Creation (80-83)
pub const OP_IMMS: u128 = 80; //   IMMS <immediate (scalar)>    ; no stack args ; result = [TOS: Scalar] ; Push immediate Scalar value on stack
pub const OP_IMML: u128 = 81; //   IMML <immediate (label)>     ; no stack args ; result = [TOS: Label] ; Push immediate Label value on stack
pub const OP_ZEROS: u128 = 82; //  ZEROS <pos>                  ; stack args = [TOS - pos: Vector|Labels] ; result = [TOS: Vector] ; Create Vector of zeros matching length of Labels at [T-pos]. Pushes on TOS. Does not consume the operand.
pub const OP_ONES: u128 = 83; //   ONES <pos>                   ; stack args = [TOS - pos: Vector|Labels] ; result = [TOS: Vector] ; Create Vector of ones matching length of Labels at [T-pos]. Pushes on TOS. Does not consume the operand.

// 9. Stack Control & Program Flow (90-94)
pub const OP_POPN: u128 = 90; //   POPN <count>                 ; stack args = ['B..., TOS - count, ..., TOS]; result = ['B...] ; Pop 'n' values from the stack
pub const OP_SWAP: u128 = 91; //   SWAP <pos>                   ; stack args = [TOS - pos: 'A, TOS: 'B] ; result = [TOS - pos: 'B, TOS: 'A]; Swap TOS with operand at [T-n]
pub const OP_B: u128 = 92; //      B <prg_id> <N> <M> <R>       ; stack args = [TOS - N] ; result = [TOS - M] ; Call sub-routine stored as Lables at `prg_id`, supplying `N` inputs and taking `M` outputs from stack. `N` inputs are consumed from stack. `M` outputs are moved from sub-routine's TOS to caller's TOS.
pub const OP_FOLD: u128 = 93; //   FOLD <prg_id> <N> <M> <R>    ; stack args = [(TOS - N - 1, ..., TOS - 1): 'A..., TOS: 'X] ; result = [TOS - M, ..., TOS] ; first iteration = [(TOS - N - 1, ..., TOS - 1): 'A..., TOS: 'X[1]] ; i-th iteration = ['R..., TOS: 'X[i]], where 'R... stack resulting from previous iteration; Fold (iterate) over vector/label operands. Same as `B` except sub-routine is called repeatedly over components of Vector at TOS.
