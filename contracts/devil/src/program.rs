use core::{clone, cmp::Ordering, mem::swap};

use alloc::vec::Vec;
use alloy_primitives::U128;
use deli::{amount::Amount, labels::Labels, vector::Vector};

pub enum ErrorCode {
    StackUnderflow,
    StackOverflow,
    InvalidInstruction,
    InvalidOperand,
    NotFound,
    OutOfRange,
    NotAligned,
    MathUnderflow,
    MathOverflow,
}

pub trait VectorIO {
    fn load_labels(&self, id: U128) -> Result<Labels, ErrorCode>;
    fn load_vector(&self, id: U128) -> Result<Vector, ErrorCode>;
    fn load_scalar(&self, id: U128) -> Result<Amount, ErrorCode>;

    fn store_labels(&mut self, id: U128, input: Labels) -> Result<(), ErrorCode>;
    fn store_vector(&mut self, id: U128, input: Vector) -> Result<(), ErrorCode>;
    fn store_scalar(&mut self, id: U128, input: Amount) -> Result<(), ErrorCode>;
}

pub struct Program<'vio, VIO>
where
    VIO: VectorIO,
{
    vio: &'vio mut VIO,
}

//
// Compute asset quantities for quantity of an index
//
// LDS IndexQuantity
// LDV AssetWeights
// MULS 1
// STV AssetQuantities
//

//
// Compute index price from asset prices
//
// LDV AssetPrice
// LDV AssetWeights
// MULV 1
// SUM
// STS IndexPrice
//

//
// Compute index slope from assets slopes
//
// LDV AssetSlope
// LDV AssetWeights
// LDD  0
// MULV 0 ; AssetWeights^2
// MULV 1
// SUM
// STS IndexSlope
//

//
// Compute index quote from assets liquidity
//
// LDV AssetWeights
// LDV AssetLiquidity
// DIVV 1
// MIN
// STS IndexCapacity
//

//
// Compute fill distribution
//
// LDV ExectutedAssetQuantities
// LDV AssetWeights ; Index 1
// LDV AssetWeights ; Index 2
// LDV AssetWeights ; Index 3
// LDD 3
// DIVV 3
// MIN ; Index 1 max possible
// LDD 4
// DIVV 3
// MIN ; Index 2 max possible
// LDD 5
// DIVV 3
// MIN ; Index 3 max possible
// LDVS 3 ; Fold scalars into vector [Index 1, Index 2, Index3 max possible]
// LDS IndexQuantity ; Index 1
// LDS IndexQuantity ; Index 2
// LDS IndexQuantity ; Index 3
// LDVS 3 ; Fold into vector [IndexQuantity Index1, Index2, Index3]
// MINV 1 ; TODO: min of vector components of two zipped vectors
// LDD 0
// SUM
// LDD 1
// DIVV 1 ; Vector of fill fractions for each index
// MULV 2 ; Vector of index quantities filled
// LDD 0
// STV FilledIndexQuantities ; a vector [FilledIndexQuantity Index1, Index2, Index3]
// STVS 0 ; Expand vector into scalars
// LDD 8
// MULS 3
// STV AssignedAssetQuantities Index 1
// LDD 7
// MULS 2
// STV AssignedAssetQuantities Index 2
// LDD 6
// MULS 1
// STV AssignedAssetQuantities Index 3
//

const OP_LDL: u128 = 1; // Load labels from VIO
const OP_LDV: u128 = 2; // Load vector from VIO
const OP_LDS: u128 = 3; // Load scalar from VIO
const OP_LDD: u128 = 4; // Load duplicate from stack
const OP_LDR: u128 = 5; // Load duplicate from registry
const OP_PKV: u128 = 6; // Pack values from stack into vector
const OP_PKL: u128 = 7; // Pack values from stack into labels
const OP_STL: u128 = 8; // Store labels into VIO
const OP_STV: u128 = 9; // Store vector into VIO
const OP_STS: u128 = 10; // Store scalar into VIO
const OP_STR: u128 = 11; // Store into registry
const OP_UNPK: u128 = 12; // Unpack values into stack
const OP_ADDV: u128 = 13; // Add vectors
const OP_ADD: u128 = 14; // Add scalar
const OP_SUBV: u128 = 15; // Subtract vectors
const OP_SUB: u128 = 16; // Subtract scalar
const OP_MULV: u128 = 17; // Multiply vectors component-wise
const OP_MUL: u128 = 18; // Multiply by scalar
const OP_DIVV: u128 = 19; // Divide vectors component-wise
const OP_DIV: u128 = 20; // Divide by scalar
const OP_SQRTV: u128 = 21; // Component-wise vector square root
const OP_SQRT: u128 = 22; // Scalar square root
const OP_SUM: u128 = 23; // Sum of vector components
const OP_MIN: u128 = 24; // Min of vector components
const OP_MAX: u128 = 25; // Max of vector components
const OP_MINV: u128 = 26; // Component-wise vector min
const OP_MAXV: u128 = 27; // Component-wise vector max
const OP_LUNN: u128 = 28; // Union of labels
const OP_ZEROS: u128 = 29; // Vector of zeros matching length of labels
const OP_ONES: u128 = 30; // Vector of ones matching length of labels
const OP_IMMS: u128 = 31; // Push immediate scalar on stack
const OP_IMML: u128 = 32; // Push immediate label on stack
const OP_VPUSH: u128 = 33; // Push scalar into vector
const OP_LPUSH: u128 = 34; // Push label into labels
const OP_VPOP: u128 = 35; // Pop scalar from vector
const OP_LPOP: u128 = 36; // Pop label from labels
const OP_POP: u128 = 37; // Pop values from stack
const OP_SWAP: u128 = 38; // Swap values on stack
const OP_JADD: u128 = 39; // Left outer join values add using labels
const OP_B: u128 = 40; // Branch unconditionally
const OP_FOLD: u128 = 41; // Fold over elements

enum Operand {
    None,
    Labels(Labels),
    Vector(Vector),
    Scalar(Amount),
    Label(u128),
}

impl Clone for Operand {
    fn clone(&self) -> Self {
        match self {
            Operand::None => Operand::None,
            Operand::Labels(x) => Operand::Labels(Labels {
                data: x.data.clone(),
            }),
            Operand::Vector(x) => Operand::Vector(Vector {
                data: x.data.clone(),
            }),
            Operand::Scalar(x) => Operand::Scalar(x.clone()),
            Operand::Label(x) => Operand::Label(x.clone()),
        }
    }
}

struct Stack {
    stack: Vec<Operand>,
    registry: Vec<Operand>,
}

impl Stack {
    fn new(num_registers: usize) -> Self {
        let mut registry = Vec::new();
        registry.resize_with(num_registers, || Operand::None);
        Self {
            stack: Vec::new(),
            registry,
        }
    }

    fn push(&mut self, operand: Operand) {
        self.stack.push(operand);
    }

    fn pop(&mut self) -> Result<Operand, ErrorCode> {
        let res = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        Ok(res)
    }

    fn ldd(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let v = self.stack.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        self.push(v.clone());
        Ok(())
    }

    fn ldr(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let v = self
            .registry
            .get(pos)
            .ok_or_else(|| ErrorCode::OutOfRange)?;
        self.push(v.clone());
        Ok(())
    }

    fn op_str(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let x = self
            .registry
            .get_mut(pos)
            .ok_or_else(|| ErrorCode::OutOfRange)?;
        *x = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        Ok(())
    }

    fn pkv(&mut self, count: usize) -> Result<(), ErrorCode> {
        let pos = self
            .stack
            .len()
            .checked_sub(count)
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let mut res = Vector::new();
        for v in self.stack.drain(pos..) {
            match v {
                Operand::Scalar(x) => {
                    res.data.push(x);
                }
                _ => Err(ErrorCode::InvalidOperand)?,
            }
        }
        self.push(Operand::Vector(res));
        Ok(())
    }

    fn pkl(&mut self, count: usize) -> Result<(), ErrorCode> {
        let pos = self
            .stack
            .len()
            .checked_sub(count)
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let mut res = Labels::new();
        for v in self.stack.drain(pos..) {
            match v {
                Operand::Label(x) => {
                    res.data.push(x);
                }
                _ => Err(ErrorCode::InvalidOperand)?,
            }
        }
        self.push(Operand::Labels(res));
        Ok(())
    }

    fn unpk(&mut self) -> Result<(), ErrorCode> {
        let v = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        let mut exp = Vec::new();
        match v {
            Operand::Vector(v) => {
                for x in v.data {
                    exp.push(Operand::Scalar(x));
                }
            }
            Operand::Labels(v) => {
                for x in v.data {
                    exp.push(Operand::Label(x));
                }
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        self.stack.extend(exp);
        Ok(())
    }

    fn addv(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let last_index = self
            .stack
            .len()
            .checked_sub(1)
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        if pos == last_index {
            // Safe to unwrap because we know the stack isn't empty
            let v1 = self.stack.last_mut().unwrap();
            match v1 {
                Operand::Vector(ref mut v1) => {
                    for i in 0..v1.data.len() {
                        let x = &mut v1.data[i];
                        *x = x.checked_add(*x).ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                }
                _ => return Err(ErrorCode::InvalidOperand),
            }
        } else {
            let (v1, rest) = self
                .stack
                .split_last_mut()
                .ok_or_else(|| ErrorCode::StackUnderflow)?;
            let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
            match (v1, v2) {
                (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                    if v1.data.len() != v2.data.len() {
                        Err(ErrorCode::NotAligned)?;
                    }
                    for i in 0..v1.data.len() {
                        let x1 = &mut v1.data[i];
                        let x2 = v2.data[i];
                        *x1 = x1.checked_add(x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                }
                _ => {
                    Err(ErrorCode::InvalidOperand)?;
                }
            }
        }
        Ok(())
    }
    
    fn add(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        match (v1, v2) {
            (Operand::Vector(ref mut v1), Operand::Scalar(ref x2)) => {
                for i in 0..v1.data.len() {
                    let x1 = &mut v1.data[i];
                    *x1 = x1.checked_add(*x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                }
            }
            (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                *x1 = (*x1)
                    .checked_add(*x2)
                    .ok_or_else(|| ErrorCode::MathOverflow)?;
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn subv(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let last_index = self
            .stack
            .len()
            .checked_sub(1)
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        if pos == last_index {
            // Safe to unwrap because we know the stack isn't empty
            let v1 = self.stack.last_mut().unwrap();
            match v1 {
                Operand::Vector(ref mut v1) => {
                    for i in 0..v1.data.len() {
                        v1.data[i] = Amount::ZERO;
                    }
                }
                _ => return Err(ErrorCode::InvalidOperand),
            }
        } else {
            let (v1, rest) = self
                .stack
                .split_last_mut()
                .ok_or_else(|| ErrorCode::StackUnderflow)?;
            let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
            match (v1, v2) {
                (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                    if v1.data.len() != v2.data.len() {
                        Err(ErrorCode::NotAligned)?;
                    }
                    for i in 0..v1.data.len() {
                        let x1 = &mut v1.data[i];
                        let x2 = v2.data[i];
                        *x1 = x1.checked_sub(x2).ok_or_else(|| ErrorCode::MathUnderflow)?;
                    }
                }
                _ => {
                    Err(ErrorCode::InvalidOperand)?;
                }
            }
        }
        Ok(())
    }

    fn sub(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        match (v1, v2) {
            (Operand::Vector(ref mut v1), Operand::Scalar(ref x2)) => {
                for i in 0..v1.data.len() {
                    let x1 = &mut v1.data[i];
                    *x1 = x1.checked_sub(*x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                }
            }
            (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                *x1 = (*x1)
                    .checked_sub(*x2)
                    .ok_or_else(|| ErrorCode::MathOverflow)?;
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn mulv(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let last_index = self
            .stack
            .len()
            .checked_sub(1)
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        if pos == last_index {
            // Safe to unwrap because we know the stack isn't empty
            let v1 = self.stack.last_mut().unwrap();
            match v1 {
                Operand::Vector(ref mut v1) => {
                    for i in 0..v1.data.len() {
                        let x = &mut v1.data[i];
                        *x = x.checked_mul(*x).ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                }
                _ => return Err(ErrorCode::InvalidOperand),
            }
        } else {
            let (v1, rest) = self
                .stack
                .split_last_mut()
                .ok_or_else(|| ErrorCode::StackUnderflow)?;
            let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
            match (v1, v2) {
                (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                    if v1.data.len() != v2.data.len() {
                        Err(ErrorCode::NotAligned)?;
                    }
                    for i in 0..v1.data.len() {
                        let x1 = &mut v1.data[i];
                        let x2 = v2.data[i];
                        *x1 = x1.checked_mul(x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                }
                _ => {
                    Err(ErrorCode::InvalidOperand)?;
                }
            }
        }
        Ok(())
    }

    fn mul(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        match (v1, v2) {
            (Operand::Vector(ref mut v1), Operand::Scalar(ref x2)) => {
                for i in 0..v1.data.len() {
                    let x1 = &mut v1.data[i];
                    *x1 = x1.checked_mul(*x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                }
            }
            (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                *x1 = (*x1)
                    .checked_mul(*x2)
                    .ok_or_else(|| ErrorCode::MathOverflow)?;
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn divv(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let last_index = self
            .stack
            .len()
            .checked_sub(1)
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        if pos == last_index {
            // Safe to unwrap because we know the stack isn't empty
            let v1 = self.stack.last_mut().unwrap();
            match v1 {
                Operand::Vector(ref mut v1) => {
                    for i in 0..v1.data.len() {
                        v1.data[i] = Amount::ONE;
                    }
                }
                _ => return Err(ErrorCode::InvalidOperand),
            }
        } else {
            let (v1, rest) = self
                .stack
                .split_last_mut()
                .ok_or_else(|| ErrorCode::StackUnderflow)?;
            let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
            match (v1, v2) {
                (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                    if v1.data.len() != v2.data.len() {
                        Err(ErrorCode::NotAligned)?;
                    }
                    for i in 0..v1.data.len() {
                        let x1 = &mut v1.data[i];
                        let x2 = v2.data[i];
                        *x1 = x1.checked_div(x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                }
                _ => {
                    Err(ErrorCode::InvalidOperand)?;
                }
            }
        }
        Ok(())
    }

    fn div(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        match (v1, v2) {
            (Operand::Vector(ref mut v1), Operand::Scalar(ref x2)) => {
                for i in 0..v1.data.len() {
                    let x1 = &mut v1.data[i];
                    *x1 = x1.checked_div(*x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                }
            }
            (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                *x1 = (*x1)
                    .checked_div(*x2)
                    .ok_or_else(|| ErrorCode::MathOverflow)?;
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn sqrtv(&mut self) -> Result<(), ErrorCode> {
        let v1 = self
            .stack
            .last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v1 {
            Operand::Vector(ref mut v1) => {
                for i in 0..v1.data.len() {
                    let x = &mut v1.data[i];
                    *x = x.checked_sqrt().ok_or_else(|| ErrorCode::MathOverflow)?;
                }
            }
            _ => return Err(ErrorCode::InvalidOperand),
        }
        Ok(())
    }

    fn sqrt(&mut self) -> Result<(), ErrorCode> {
        let v1 = self
            .stack
            .last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v1 {
            Operand::Scalar(ref mut x) => {
                *x = x.checked_sqrt().ok_or_else(|| ErrorCode::MathOverflow)?;
            }
            _ => return Err(ErrorCode::InvalidOperand),
        }
        Ok(())
    }

    fn sum(&mut self) -> Result<(), ErrorCode> {
        let v = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Vector(ref v) => {
                let mut s = Amount::ZERO;
                for i in 0..v.data.len() {
                    let x = v.data[i];
                    s = s.checked_add(x).ok_or_else(|| ErrorCode::MathOverflow)?;
                }
                self.stack.push(Operand::Scalar(s));
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn min(&mut self) -> Result<(), ErrorCode> {
        let v = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Vector(ref v) => {
                let mut s = Amount::ZERO;
                for i in 0..v.data.len() {
                    let x = v.data[i];
                    s = s.min(x);
                }
                self.stack.push(Operand::Scalar(s));
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn max(&mut self) -> Result<(), ErrorCode> {
        let v = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Vector(ref v) => {
                let mut s = Amount::ZERO;
                for i in 0..v.data.len() {
                    let x = v.data[i];
                    s = s.max(x);
                }
                self.stack.push(Operand::Scalar(s));
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn minv(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        match (v1, v2) {
            (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                if v1.data.len() != v2.data.len() {
                    Err(ErrorCode::NotAligned)?;
                }
                for i in 0..v1.data.len() {
                    let x1 = &mut v1.data[i];
                    let x2 = v2.data[i];
                    *x1 = (*x1).min(x2);
                }
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn maxv(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        match (v1, v2) {
            (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                if v1.data.len() != v2.data.len() {
                    Err(ErrorCode::NotAligned)?;
                }
                for i in 0..v1.data.len() {
                    let x1 = &mut v1.data[i];
                    let x2 = v2.data[i];
                    *x1 = (*x1).max(x2);
                }
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn lunn(&mut self, other_labels_pos: usize) -> Result<(), ErrorCode> {
        let current_len = self.stack.len();
        if current_len < 1 {
            return Err(ErrorCode::StackUnderflow);
        }

        // --- 1. Calculate Absolute Indices and Borrow Labels ---

        // Labels A is at the top of the stack (pos 0 relative to end)
        let last_idx = current_len - 1;

        // Labels B is at other_labels_pos relative to the top
        let other_idx = last_idx
            .checked_sub(other_labels_pos)
            .ok_or(ErrorCode::OutOfRange)?;

        // Check that we're not trying to union an item with itself
        if last_idx == other_idx {
            // Simple case: Unioning with self is just the original Labels.
            // We still need to copy and push the result to the stack top.
            // We will fall through to the copy logic after the main merge.
        }

        // Borrow Labels A (last) and Labels B (other) immutably
        let labels_a_op = self.stack.get(last_idx).ok_or(ErrorCode::OutOfRange)?;
        let labels_b_op = self.stack.get(other_idx).ok_or(ErrorCode::OutOfRange)?;

        let labels_a = match labels_a_op {
            Operand::Labels(l) => l,
            _ => return Err(ErrorCode::InvalidOperand),
        };
        let labels_b = match labels_b_op {
            Operand::Labels(l) => l,
            _ => return Err(ErrorCode::InvalidOperand),
        };

        // --- 2. Perform the Sorted Union Merge ---

        let mut result_data: Vec<u128> =
            Vec::with_capacity(labels_a.data.len() + labels_b.data.len());

        let mut i = 0; // Pointer for A (Last)
        let mut j = 0; // Pointer for B (Other)

        while i < labels_a.data.len() || j < labels_b.data.len() {
            // Safe access to current label in A and B
            let label_a = labels_a.data.get(i);
            let label_b = labels_b.data.get(j);

            let next_label: &u128;

            match (label_a, label_b) {
                (Some(la), Some(lb)) => {
                    match la.cmp(lb) {
                        Ordering::Less => {
                            // Case 1: Label A is smaller. Take A. Advance A.
                            next_label = la;
                            i += 1;
                        }
                        Ordering::Equal => {
                            // Case 2: Match. Take one (A). Advance both.
                            next_label = la;
                            i += 1;
                            j += 1;
                        }
                        Ordering::Greater => {
                            // Case 3: Label B is smaller. Take B. Advance B.
                            next_label = lb;
                            j += 1;
                        }
                    }
                }
                (Some(la), None) => {
                    // Case 4: Reached end of B. Take A. Advance A.
                    next_label = la;
                    i += 1;
                }
                (None, Some(lb)) => {
                    // Case 5: Reached end of A. Take B. Advance B.
                    next_label = lb;
                    j += 1;
                }
                (None, None) => break,
            }

            result_data.push(*next_label);
        }

        // 3. Push the new Labels (C) onto the stack
        self.stack
            .push(Operand::Labels(Labels { data: result_data }));

        Ok(())
    }

    fn zeros(&mut self, labels_pos: usize) -> Result<(), ErrorCode> {
        let current_len = self.stack.len();
        if current_len == 0 {
            return Err(ErrorCode::StackUnderflow);
        }

        // --- 1. Calculate Absolute Index and Borrow Labels ---
        // labels_pos is relative to the stack end (len - 1).
        let last_idx = current_len - 1;
        let abs_idx = last_idx
            .checked_sub(labels_pos)
            .ok_or(ErrorCode::OutOfRange)?;

        // Borrow the Labels operand immutably
        let labels_op = self.stack.get(abs_idx).ok_or(ErrorCode::OutOfRange)?;

        // 2. Extract the length from the Labels operand
        let labels_len = match labels_op {
            Operand::Labels(l) => l.data.len(),
            _ => return Err(ErrorCode::InvalidOperand), // Must be a Labels operand
        };

        // 3. Create the Default Vector (filled with Amount::ZERO)
        // Vector::new(len, Amount::ZERO) is conceptually what we're doing.
        let default_data = vec![Amount::ZERO; labels_len];

        // 4. Push the new Vector onto the stack
        self.stack
            .push(Operand::Vector(Vector { data: default_data }));

        Ok(())
    }

    fn ones(&mut self, labels_pos: usize) -> Result<(), ErrorCode> {
        let current_len = self.stack.len();
        if current_len == 0 {
            return Err(ErrorCode::StackUnderflow);
        }

        // 1. Calculate Absolute Index and Borrow Labels
        let last_idx = current_len - 1;
        let abs_idx = last_idx
            .checked_sub(labels_pos)
            .ok_or(ErrorCode::OutOfRange)?;

        let labels_op = self.stack.get(abs_idx).ok_or(ErrorCode::OutOfRange)?;

        // 2. Extract the length from the Labels operand
        let labels_len = match labels_op {
            Operand::Labels(l) => l.data.len(),
            _ => return Err(ErrorCode::InvalidOperand),
        };

        // 3. Create the Vector of Ones
        // Note: This relies on Amount::ONE being available in scope.
        let one_data = vec![Amount::ONE; labels_len];

        // 4. Push the new Vector onto the stack
        self.stack.push(Operand::Vector(Vector { data: one_data }));

        Ok(())
    }

    fn imms(&mut self, value: u128) -> Result<(), ErrorCode> {
        self.push(Operand::Scalar(Amount::from_u128_raw(value)));
        Ok(())
    }

    fn imml(&mut self, value: u128) -> Result<(), ErrorCode> {
        self.push(Operand::Label(value));
        Ok(())
    }

    fn vpush(&mut self, value: u128) -> Result<(), ErrorCode> {
        let v = self
            .stack
            .last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Vector(ref mut v) => {
                v.data.push(Amount::from_u128_raw(value));
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }
        Ok(())
    }

    fn lpush(&mut self, value: u128) -> Result<(), ErrorCode> {
        let v = self
            .stack
            .last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Labels(ref mut v) => {
                v.data.push(value);
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }
        Ok(())
    }

    fn vpop(&mut self) -> Result<(), ErrorCode> {
        let v = self
            .stack
            .last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Vector(ref mut v) => {
                let val = v.data.pop().ok_or_else(|| ErrorCode::OutOfRange)?;
                self.stack.push(Operand::Scalar(val));
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }
        Ok(())
    }

    fn lpop(&mut self) -> Result<(), ErrorCode> {
        let v = self
            .stack
            .last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Labels(ref mut v) => {
                let val = v.data.pop().ok_or_else(|| ErrorCode::OutOfRange)?;
                self.stack.push(Operand::Label(val));
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }
        Ok(())
    }

    fn op_pop(&mut self, count: usize) -> Result<(), ErrorCode> {
        if self.stack.len() < count {
            Err(ErrorCode::StackUnderflow)?;
        }
        for _ in self.stack.drain(0..count) {}
        Ok(())
    }

    fn swap(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get_mut(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        swap(v1, v2);
        Ok(())
    }

    fn jadd(&mut self, mut pos_labels_a: usize, mut pos_labels_b: usize) -> Result<(), ErrorCode> {
        // Current stack size before popping is self.stack.len().
        let original_len = self.stack.len();

        if original_len < 2 {
            return Err(ErrorCode::StackUnderflow);
        }

        // --- 1. Pop Vectors A and B (Destructive Read) ---
        // Stack is reduced by 2 here. The positions of the labels are now incorrect.
        let vector_a = match self.stack.pop().unwrap() {
            Operand::Vector(v) => v,
            _ => return Err(ErrorCode::InvalidOperand),
        };
        let vector_b = match self.stack.pop().unwrap() {
            Operand::Vector(v) => v,
            _ => return Err(ErrorCode::InvalidOperand),
        };

        // --- 2. Correct Positions for Non-Destructive Label Read ---
        // The relative position of the labels has shifted by 2 towards the top.
        pos_labels_a = pos_labels_a.checked_sub(2).ok_or(ErrorCode::OutOfRange)?;
        pos_labels_b = pos_labels_b.checked_sub(2).ok_or(ErrorCode::OutOfRange)?;

        // --- 3. Calculate Absolute Indices and Borrow Labels (Non-Destructive Read) ---
        let abs_idx_labels_a = self
            .stack
            .len()
            .checked_sub(1)
            .unwrap()
            .checked_sub(pos_labels_a)
            .ok_or(ErrorCode::OutOfRange)?;
        let abs_idx_labels_b = self
            .stack
            .len()
            .checked_sub(1)
            .unwrap()
            .checked_sub(pos_labels_b)
            .ok_or(ErrorCode::OutOfRange)?;

        let labels_a_op = self
            .stack
            .get(abs_idx_labels_a)
            .ok_or(ErrorCode::OutOfRange)?;
        let labels_b_op = self
            .stack
            .get(abs_idx_labels_b)
            .ok_or(ErrorCode::OutOfRange)?;

        let labels_a = match labels_a_op {
            Operand::Labels(l) => l,
            _ => return Err(ErrorCode::InvalidOperand),
        };
        let labels_b = match labels_b_op {
            Operand::Labels(l) => l,
            _ => return Err(ErrorCode::InvalidOperand),
        };

        // 4. Alignment Check: Labels must match their respective Vector length
        if labels_a.data.len() != vector_a.data.len() || labels_b.data.len() != vector_b.data.len()
        {
            return Err(ErrorCode::NotAligned);
        }

        // --- 5. Left Outer Merge Join Loop ---

        let mut result_data: Vec<Amount> = Vec::with_capacity(vector_a.data.len());

        let mut i = 0; // Pointer for A (Left)
        let mut j = 0; // Pointer for B (Right)

        // Iterate until all of Labels A have been processed (preserves length of A)
        while i < labels_a.data.len() {
            let label_a = &labels_a.data[i];
            let value_a = vector_a.data[i];

            let result_value = if j < labels_b.data.len() {
                let label_b = &labels_b.data[j];

                match label_a.cmp(label_b) {
                    Ordering::Less => {
                        // Case 1: Label A is smaller (no match in B). Result = A + 0.
                        i += 1;
                        value_a
                    }
                    Ordering::Equal => {
                        // Case 2: Match found. Perform the ADD operation.
                        let value_b = vector_b.data[j];
                        let added = value_a
                            .checked_add(value_b)
                            .ok_or(ErrorCode::MathOverflow)?;
                        i += 1;
                        j += 1;
                        added
                    }
                    Ordering::Greater => {
                        // Case 3: Label B is smaller (B has extra data). Skip B (Left Join).
                        j += 1;
                        continue; // Re-check A[i] against the next B[j]
                    }
                }
            } else {
                // Case 4: Reached end of Labels B. All remaining A values are unmatched. Result = A + 0.
                i += 1;
                value_a
            };

            result_data.push(result_value);
        }

        // 6. Push the result Vector C (The new Labels are implicitly Labels A)
        self.stack
            .push(Operand::Vector(Vector { data: result_data }));

        Ok(())
    }
}

impl<'vio, VIO> Program<'vio, VIO>
where
    VIO: VectorIO,
{
    pub fn new(vio: &'vio mut VIO) -> Self {
        Self { vio }
    }

    pub fn execute(&mut self, code_bytes: Vec<u8>, num_registers: usize) -> Result<(), ErrorCode> {
        let code = Labels::from_vec(code_bytes).data;
        let mut stack = Stack::new(num_registers);
        self.execute_with_stack(code, &mut stack)
    }

    fn execute_with_stack(&mut self, code: Vec<u128>, stack: &mut Stack) -> Result<(), ErrorCode> {
        let mut pc = 0;
        while pc < code.len() {
            let op_code = code[pc];
            pc += 1;
            match op_code {
                OP_LDL => {
                    let id = code[pc];
                    pc += 1;
                    let v = self.vio.load_labels(U128::from(id))?;
                    stack.push(Operand::Labels(v));
                }
                OP_LDV => {
                    let id = code[pc];
                    pc += 1;
                    let v = self.vio.load_vector(U128::from(id))?;
                    stack.push(Operand::Vector(v));
                }
                OP_LDS => {
                    let id = code[pc];
                    pc += 1;
                    let v = self.vio.load_scalar(U128::from(id))?;
                    stack.push(Operand::Scalar(v));
                }
                OP_STL => {
                    let id = code[pc];
                    pc += 1;
                    match stack.pop()? {
                        Operand::Labels(v) => {
                            self.vio.store_labels(U128::from(id), v)?;
                        }
                        _ => {
                            Err(ErrorCode::InvalidOperand)?;
                        }
                    }
                }
                OP_STV => {
                    let id = code[pc];
                    pc += 1;
                    match stack.pop()? {
                        Operand::Vector(v) => {
                            self.vio.store_vector(U128::from(id), v)?;
                        }
                        _ => {
                            Err(ErrorCode::InvalidOperand)?;
                        }
                    }
                }
                OP_STS => {
                    let id = code[pc];
                    pc += 1;
                    match stack.pop()? {
                        Operand::Scalar(v) => {
                            self.vio.store_scalar(U128::from(id), v)?;
                        }
                        _ => {
                            Err(ErrorCode::InvalidOperand)?;
                        }
                    }
                }
                OP_LDD => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.ldd(pos)?;
                }
                OP_LDR => {
                    let reg = code[pc] as usize;
                    pc += 1;
                    stack.ldr(reg)?;
                }
                OP_STR => {
                    let reg = code[pc] as usize;
                    pc += 1;
                    stack.op_str(reg)?;
                }
                OP_PKV => {
                    let count = code[pc] as usize;
                    pc += 1;
                    stack.pkv(count)?;
                }
                OP_PKL => {
                    let count = code[pc] as usize;
                    pc += 1;
                    stack.pkl(count)?;
                }
                OP_UNPK => {
                    stack.unpk()?;
                }
                OP_ADDV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.addv(pos)?;
                }
                OP_ADD => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.add(pos)?;
                }
                OP_SUBV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.subv(pos)?;
                }
                OP_SUB => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.sub(pos)?;
                }
                OP_MULV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.mulv(pos)?;
                }
                OP_MUL => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.mul(pos)?;
                }
                OP_DIVV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.divv(pos)?;
                }
                OP_DIV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.div(pos)?;
                }
                OP_SQRTV => {
                    stack.sqrtv()?;
                }
                OP_SQRT => {
                    stack.sqrt()?;
                }
                OP_SUM => {
                    stack.sum()?;
                }
                OP_MIN => {
                    stack.min()?;
                }
                OP_MAX => {
                    stack.max()?;
                }
                OP_MINV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.minv(pos)?;
                }
                OP_MAXV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.maxv(pos)?;
                }
                OP_LUNN => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.lunn(pos)?;
                }
                OP_ZEROS => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.zeros(pos)?;
                }
                OP_ONES => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.ones(pos)?;
                }
                OP_IMMS => {
                    let val = code[pc];
                    pc += 1;
                    stack.imms(val)?;
                }
                OP_IMML => {
                    let val = code[pc];
                    pc += 1;
                    stack.imml(val)?;
                }
                OP_VPUSH => {
                    let val = code[pc];
                    pc += 1;
                    stack.vpush(val)?;
                }
                OP_LPUSH => {
                    let val = code[pc];
                    pc += 1;
                    stack.lpush(val)?;
                }
                OP_VPOP => {
                    stack.vpop()?;
                }
                OP_LPOP => {
                    stack.lpop()?;
                }
                OP_POP => {
                    let count = code[pc] as usize;
                    pc += 1;
                    stack.op_pop(count)?;
                }
                OP_SWAP => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.swap(pos)?;
                }
                OP_JADD => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    stack.jadd(pos_1, pos_2)?;
                }
                OP_B => {
                    // B <program_id> <num_inputs> <num_outputs> <num_registers>
                    let code_address = U128::from(code[pc]);
                    pc += 1;
                    let num_inputs = code[pc] as usize;
                    pc += 1;
                    let num_outputs = code[pc] as usize;
                    pc += 1;
                    let num_regs = code[pc] as usize;
                    pc += 1;
                    let mut st = Stack::new(num_regs);
                    let mut prg = Program::new(self.vio);
                    let cod = prg.vio.load_labels(code_address)?;
                    let frm = stack
                        .stack
                        .len()
                        .checked_sub(num_inputs)
                        .ok_or_else(|| ErrorCode::StackUnderflow)?;
                    st.stack.extend(stack.stack.drain(frm..));
                    prg.execute_with_stack(cod.data, &mut st)?;
                    let frm = st
                        .stack
                        .len()
                        .checked_sub(num_outputs)
                        .ok_or_else(|| ErrorCode::StackUnderflow)?;
                    stack.stack.extend(st.stack.drain(frm..));
                }
                OP_FOLD => {
                    // FOLD <program_id> <num_inputs> <num_outputs> <num_registers>
                    let code_address = U128::from(code[pc]);
                    pc += 1;
                    let num_inputs = code[pc] as usize;
                    pc += 1;
                    let num_outputs = code[pc] as usize;
                    pc += 1;
                    let num_regs = code[pc] as usize;
                    pc += 1;
                    let mut st = Stack::new(num_regs);
                    let mut prg = Program::new(self.vio);
                    let cod = prg.vio.load_labels(code_address)?;
                    let source = stack.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
                    let frm = stack
                        .stack
                        .len()
                        .checked_sub(num_inputs)
                        .ok_or_else(|| ErrorCode::StackUnderflow)?;
                    st.stack.extend(stack.stack.drain(frm..));
                    match source {
                        Operand::Labels(s) => {
                            for item in s.data {
                                st.stack.push(Operand::Label(item));
                                prg.execute_with_stack(cod.data.clone(), &mut st)?;
                            }
                        }
                        Operand::Vector(s) => {
                            for item in s.data {
                                st.stack.push(Operand::Scalar(item));
                                prg.execute_with_stack(cod.data.clone(), &mut st)?;
                            }
                        }
                        _ => Err(ErrorCode::InvalidOperand)?,
                    }
                    let frm = st
                        .stack
                        .len()
                        .checked_sub(num_outputs)
                        .ok_or_else(|| ErrorCode::StackUnderflow)?;
                    stack.stack.extend(st.stack.drain(frm..));
                }
                _ => {
                    Err(ErrorCode::InvalidInstruction)?;
                }
            }
        }

        Ok(())
    }
}
