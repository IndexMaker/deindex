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

const OP_LDL: u128 = 1;
const OP_LDV: u128 = 2;
const OP_LDS: u128 = 3;
const OP_LDD: u128 = 4;
const OP_LDVS: u128 = 5;
const OP_STL: u128 = 6;
const OP_STV: u128 = 7;
const OP_STS: u128 = 8;
const OP_STVS: u128 = 9;
const OP_ADDV: u128 = 10;
const OP_SUBV: u128 = 11;
const OP_MULV: u128 = 12;
const OP_MULS: u128 = 13;
const OP_DIVV: u128 = 14;
const OP_DIVS: u128 = 15;
const OP_SUM: u128 = 16;
const OP_MIN: u128 = 17;
const OP_MAX: u128 = 18;
const OP_MINV: u128 = 19;
const OP_MAXV: u128 = 20;

enum Operand {
    Labels(Labels),
    Vector(Vector),
    Scalar(Amount),
}

struct Stack {
    stack: Vec<Operand>,
}

impl Stack {
    fn new() -> Self {
        Self { stack: Vec::new() }
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
        let z = match v {
            Operand::Labels(x) => Operand::Labels(Labels {
                data: x.data.clone(),
            }),
            Operand::Vector(x) => Operand::Vector(Vector {
                data: x.data.clone(),
            }),
            Operand::Scalar(x) => Operand::Scalar(x.clone()),
        };
        self.push(z);
        Ok(())
    }

    fn ldvs(&mut self, count: usize) -> Result<(), ErrorCode> {
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

    fn stvs(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let v = self.stack.get(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        let mut exp = Vec::new();
        match v {
            Operand::Vector(v) => {
                for x in &v.data {
                    exp.push(Operand::Scalar(*x));
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

    fn muls(&mut self, pos: usize) -> Result<(), ErrorCode> {
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

    fn divs(&mut self, pos: usize) -> Result<(), ErrorCode> {
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
}

impl<'vio, VIO> Program<'vio, VIO>
where
    VIO: VectorIO,
{
    pub fn new(vio: &'vio mut VIO) -> Self {
        Self { vio }
    }

    pub fn execute(&mut self, code_bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let code = Labels::from_vec(code_bytes).data;
        let mut stack = Stack::new();
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
                OP_LDVS => {
                    let count = code[pc] as usize;
                    pc += 1;
                    stack.ldvs(count)?;
                }
                OP_STVS => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.stvs(pos)?;
                }
                OP_ADDV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.addv(pos)?;
                }
                OP_SUBV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.subv(pos)?;
                }
                OP_MULV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.mulv(pos)?;
                }
                OP_MULS => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.muls(pos)?;
                }
                OP_DIVV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.divv(pos)?;
                }
                OP_DIVS => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.divs(pos)?;
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
                _ => {
                    Err(ErrorCode::InvalidInstruction)?;
                }
            }
        }

        Ok(())
    }
}
