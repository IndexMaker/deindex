use core::mem::swap;

#[cfg(test)]
use core::fmt::Debug;

use alloc::vec::Vec;
use alloy_primitives::U128;
use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector, vis::*};

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

#[cfg(test)]
impl Debug for ErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::StackUnderflow => write!(f, "StackUnderflow"),
            Self::StackOverflow => write!(f, "StackOverflow"),
            Self::InvalidInstruction => write!(f, "InvalidInstruction"),
            Self::InvalidOperand => write!(f, "InvalidOperand"),
            Self::NotFound => write!(f, "NotFound"),
            Self::OutOfRange => write!(f, "OutOfRange"),
            Self::NotAligned => write!(f, "NotAligned"),
            Self::MathUnderflow => write!(f, "MathUnderflow"),
            Self::MathOverflow => write!(f, "MathOverflow"),
        }
    }
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

macro_rules! impl_devil_binary_op {
    (
        $fn_name:ident,
        $checked_op:ident
    ) => {
        fn $fn_name(&mut self, pos: usize) -> Result<(), ErrorCode> {
            if pos == 0 {
                let v1 = self
                    .stack
                    .last_mut()
                    .ok_or_else(|| ErrorCode::StackUnderflow)?;
                match v1 {
                    Operand::Vector(ref mut v1) => {
                        for x in v1.data.iter_mut() {
                            *x = x.$checked_op(*x).ok_or_else(|| ErrorCode::MathOverflow)?;
                        }
                    }
                    Operand::Scalar(ref mut x1) => {
                        *x1 = (*x1)
                            .$checked_op(*x1)
                            .ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                    _ => return Err(ErrorCode::InvalidOperand),
                }
            } else {
                let stack_index = self.get_stack_index(pos)?;
                let (v1, rest) = self
                    .stack
                    .split_last_mut()
                    .ok_or_else(|| ErrorCode::StackUnderflow)?;

                let v2 = rest.get(stack_index).ok_or_else(|| ErrorCode::OutOfRange)?;

                match (v1, v2) {
                    (Operand::Vector(ref mut v1), Operand::Vector(ref v2)) => {
                        if v1.data.len() != v2.data.len() {
                            Err(ErrorCode::NotAligned)?;
                        }
                        for (x1, x2) in v1.data.iter_mut().zip(v2.data.iter()) {
                            *x1 = x1.$checked_op(*x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                        }
                    }
                    (Operand::Vector(ref mut v1), Operand::Scalar(ref x2)) => {
                        for x1 in v1.data.iter_mut() {
                            *x1 = x1.$checked_op(*x2).ok_or_else(|| ErrorCode::MathOverflow)?;
                        }
                    }
                    (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                        *x1 = (*x1)
                            .$checked_op(*x2)
                            .ok_or_else(|| ErrorCode::MathOverflow)?;
                    }
                    _ => {
                        Err(ErrorCode::InvalidOperand)?;
                    }
                }
            }
            Ok(())
        }
    };
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

    fn get_stack_offset(&self, count: usize) -> Result<usize, ErrorCode> {
        let depth = self.stack.len();
        if depth == 0 {
            Err(ErrorCode::StackUnderflow)?;
        }
        if depth < count {
            Err(ErrorCode::StackUnderflow)?;
        }
        Ok(depth - count)
    }

    fn get_stack_index(&self, pos: usize) -> Result<usize, ErrorCode> {
        let depth = self.stack.len();
        if depth == 0 {
            Err(ErrorCode::StackUnderflow)?;
        }
        let last_index = depth - 1;
        if last_index < pos {
            Err(ErrorCode::StackOverflow)?;
        }
        Ok(last_index - pos)
    }

    fn ldd(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let v = &self.stack[self.get_stack_index(pos)?];
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
        let pos = self.get_stack_offset(count)?;

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
        let pos = self.get_stack_offset(count)?;

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

    fn transpose(&mut self, count: usize) -> Result<(), ErrorCode> {
        if count == 0 {
            Err(ErrorCode::InvalidOperand)?;
        }

        if count == 1 {
            return self.unpk();
        }

        let pos = self.get_stack_offset(count)?;
        let mut vectors = Vec::with_capacity(count);
        for v in self.stack.drain(pos..) {
            match v {
                Operand::Vector(v) => vectors.push(v.data),
                _ => {
                    Err(ErrorCode::InvalidOperand)?;
                }
            }
        }
        let num_rows = vectors[0].len();
        for v in &vectors {
            if v.len() != num_rows {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        let mut transposed = vec![vec![Amount::ZERO; count]; num_rows];
        for row in 0..num_rows {
            for col in 0..count {
                transposed[row][col] = vectors[col][row];
            }
        }

        for v in transposed {
            self.stack.push(Operand::Vector(Vector { data: v }));
        }

        Ok(())
    }

    impl_devil_binary_op!(add, checked_add);
    impl_devil_binary_op!(sub, checked_sub);
    impl_devil_binary_op!(ssb, saturating_sub);
    impl_devil_binary_op!(mul, checked_mul);
    impl_devil_binary_op!(div, checked_div);

    fn sqrt(&mut self) -> Result<(), ErrorCode> {
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
            Operand::Scalar(ref mut x) => {
                *x = x.checked_sqrt().ok_or_else(|| ErrorCode::MathOverflow)?;
            }
            _ => return Err(ErrorCode::InvalidOperand),
        }
        Ok(())
    }

    fn vsum(&mut self) -> Result<(), ErrorCode> {
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

    fn vmin(&mut self) -> Result<(), ErrorCode> {
        let v = self.stack.pop().ok_or_else(|| ErrorCode::StackUnderflow)?;
        match v {
            Operand::Vector(ref v) => {
                let mut s = Amount::MAX;
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

    fn vmax(&mut self) -> Result<(), ErrorCode> {
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

    fn min(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let stack_index = self.get_stack_index(pos)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(stack_index).ok_or_else(|| ErrorCode::OutOfRange)?;
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
            (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                *x1 = (*x1).min(*x2);
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn max(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let stack_index = self.get_stack_index(pos)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get(stack_index).ok_or_else(|| ErrorCode::OutOfRange)?;
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
            (Operand::Scalar(ref mut x1), Operand::Scalar(ref x2)) => {
                *x1 = (*x1).max(*x2);
            }
            _ => {
                Err(ErrorCode::InvalidOperand)?;
            }
        }
        Ok(())
    }

    fn zeros(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let stack_index = self.get_stack_index(pos)?;
        let labels = self.stack.get(stack_index).ok_or(ErrorCode::OutOfRange)?;

        let num_cols = match labels {
            Operand::Vector(v) => v.data.len(),
            Operand::Labels(l) => l.data.len(),
            _ => return Err(ErrorCode::InvalidOperand), // Must be a Labels operand
        };

        self.stack.push(Operand::Vector(Vector {
            data: vec![Amount::ZERO; num_cols],
        }));

        Ok(())
    }

    fn ones(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let stack_index = self.get_stack_index(pos)?;
        let labels = self.stack.get(stack_index).ok_or(ErrorCode::OutOfRange)?;

        let num_cols = match labels {
            Operand::Vector(v) => v.data.len(),
            Operand::Labels(l) => l.data.len(),
            _ => return Err(ErrorCode::InvalidOperand), // Must be a Labels operand
        };

        self.stack.push(Operand::Vector(Vector {
            data: vec![Amount::ONE; num_cols],
        }));

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

    fn op_popn(&mut self, count: usize) -> Result<(), ErrorCode> {
        let pos = self.get_stack_offset(count)?;
        for _ in self.stack.drain(pos..) {}
        Ok(())
    }

    fn swap(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let pos = self.get_stack_index(pos)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;
        let v2 = rest.get_mut(pos).ok_or_else(|| ErrorCode::OutOfRange)?;
        swap(v1, v2);
        Ok(())
    }

    fn lunion(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let stack_index = self.get_stack_index(pos)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let v2 = rest
            .get(stack_index)
            .ok_or_else(|| ErrorCode::OutOfRange)?;

        match (v1, v2) {
            (
                Operand::Labels(labels_a),
                Operand::Labels(labels_b),
            ) => {
                let mut result = Vec::new();
                let mut j = 0;
                for i in 0..labels_a.data.len() {
                    let label_a = labels_a.data[i];
                    let mut updated = false;
                    while j < labels_b.data.len() {
                        updated = true;
                        let label_b = labels_b.data[j];
                        if label_b < label_a {
                            result.push(label_b);
                            j += 1;
                            continue;
                        } else if label_a < label_b {
                            result.push(label_a);
                            break;
                        } else {
                            result.push(label_a);
                            j += 1;
                            break;
                        }
                    }
                    if !updated {
                        result.push(label_a);
                    }
                }
                labels_a.data = result;
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }
        Ok(())
    }

    fn jadd(&mut self, pos_labels_a: usize, pos_labels_b: usize) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then it's just normal vector add
            return self.add(1);
        }
        if pos_labels_a < 2 || pos_labels_b < 2 {
            // [TOS - 1, TOS] are reserved for values
            Err(ErrorCode::InvalidOperand)?;
        }
        let stack_index_v2 = self.get_stack_index(1)?;
        let stack_index_labels_a = self.get_stack_index(pos_labels_a)?;
        let stack_index_labels_b = self.get_stack_index(pos_labels_b)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let v2 = rest
            .get(stack_index_v2)
            .ok_or_else(|| ErrorCode::OutOfRange)?;
        let labels_a = &rest[stack_index_labels_a];
        let labels_b = &rest[stack_index_labels_b];

        match (v1, v2, labels_a, labels_b) {
            (
                Operand::Vector(v1),
                Operand::Vector(v2),
                Operand::Labels(labels_a),
                Operand::Labels(labels_b),
            ) => {
                if v1.data.len() != labels_a.data.len() {
                    Err(ErrorCode::InvalidOperand)?;
                }
                if v2.data.len() != labels_b.data.len() {
                    Err(ErrorCode::InvalidOperand)?;
                }
                let mut j = 0;
                for i in 0..labels_a.data.len() {
                    let label_a = labels_a.data[i];
                    while j < labels_b.data.len() {
                        let label_b = labels_b.data[j];
                        if label_b < label_a {
                            // Label B not in A. This is an error, as we are not
                            // extending A, and value would be missing. A must
                            // have at least all same labels as B or more
                            // labels.
                            Err(ErrorCode::MathUnderflow)?
                        } else if label_a < label_b {
                            // Label A not in B.  Preserve value in A, as A + None = A
                            break;
                        } else {
                            // Found matching label in B. Sum values in-place: A <- A + B
                            let x1 = &mut v1.data[i];
                            *x1 = x1
                                .checked_add(v2.data[j])
                                .ok_or_else(|| ErrorCode::MathOverflow)?;
                            j += 1;
                            break;
                        }
                    }
                    // NOTE: if we didn't match any label in B, then preserve value in A, as A + None = A
                }
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }

        Ok(())
    }

     fn jssb(&mut self, pos_labels_a: usize, pos_labels_b: usize) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then it's just normal vector add
            return self.add(1);
        }
        if pos_labels_a < 2 || pos_labels_b < 2 {
            // [TOS - 1, TOS] are reserved for values
            Err(ErrorCode::InvalidOperand)?;
        }
        let stack_index_v2 = self.get_stack_index(1)?;
        let stack_index_labels_a = self.get_stack_index(pos_labels_a)?;
        let stack_index_labels_b = self.get_stack_index(pos_labels_b)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let v2 = rest
            .get(stack_index_v2)
            .ok_or_else(|| ErrorCode::OutOfRange)?;
        let labels_a = &rest[stack_index_labels_a];
        let labels_b = &rest[stack_index_labels_b];

        match (v1, v2, labels_a, labels_b) {
            (
                Operand::Vector(v1),
                Operand::Vector(v2),
                Operand::Labels(labels_a),
                Operand::Labels(labels_b),
            ) => {
                if v1.data.len() != labels_a.data.len() {
                    Err(ErrorCode::InvalidOperand)?;
                }
                if v2.data.len() != labels_b.data.len() {
                    Err(ErrorCode::InvalidOperand)?;
                }
                let mut j = 0;
                for i in 0..labels_a.data.len() {
                    let label_a = labels_a.data[i];
                    while j < labels_b.data.len() {
                        let label_b = labels_b.data[j];
                        if label_b < label_a {
                            // Label B not in A. This is an error, as we are not
                            // extending A, and value would be negative. A must
                            // have at least all same labels as B or more
                            // labels.
                            Err(ErrorCode::MathUnderflow)?
                        } else if label_a < label_b {
                            // Label A not in B.  Preserve value in A, as A - None = A
                            break;
                        } else {
                            // Found matching label in B. Subtract values in-place: A <- A - MIN(A, B)
                            let x1 = &mut v1.data[i];
                            let x2 = v2.data[j];
                            
                            *x1 = x1.saturating_sub(x2)
                                .ok_or_else(|| ErrorCode::MathOverflow)?;
                            j += 1;
                            break;
                        }
                    }
                    // NOTE: if we didn't match any label in B, then preserve value in A, as A - None = A
                }
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }

        Ok(())
    }

    fn jxpnd(&mut self, pos_labels_a: usize, pos_labels_b: usize) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then no work needed
            return Ok(())
        }
        if pos_labels_a < 2 || pos_labels_b < 2 {
            // [TOS - 1, TOS] are reserved for values
            Err(ErrorCode::InvalidOperand)?;
        }
        let stack_index_labels_a = self.get_stack_index(pos_labels_a)?;
        let stack_index_labels_b = self.get_stack_index(pos_labels_b)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let labels_a = &rest[stack_index_labels_a];
        let labels_b = &rest[stack_index_labels_b];

        match (v1, labels_a, labels_b) {
            (
                Operand::Vector(v1),
                Operand::Labels(labels_a),
                Operand::Labels(labels_b),
            ) => {
                if v1.data.len() != labels_a.data.len() {
                    Err(ErrorCode::InvalidOperand)?;
                }
                let mut j = 0;
                let mut k = 0;
                for i in 0..labels_a.data.len() {
                    let label_a = labels_a.data[i];
                    let mut updated = false;
                    while j < labels_b.data.len() {
                        updated = true;
                        let label_b = labels_b.data[j];
                        if label_b < label_a {
                            // Label B not in A.
                            v1.data.insert(k, Amount::ZERO);
                            k += 1;
                            // we continue with same B, and next A
                            continue;
                        } else if label_a < label_b {
                            // Label A not in B. A must be a subset of B.
                            Err(ErrorCode::NotFound)?;
                        } else {
                            // Found matching label in B.
                            j += 1;
                            k += 1;
                            // go to next A and next B
                            break;
                        }
                    }
                    if !updated {
                        // Label A not in B. A must be a subset of B.
                        Err(ErrorCode::NotFound)?;
                    }
                }
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }

        Ok(())
    }
    
    fn jfltr(&mut self, pos_labels_a: usize, pos_labels_b: usize) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then no work needed
            return Ok(())
        }
        if pos_labels_a < 2 || pos_labels_b < 2 {
            // [TOS - 1, TOS] are reserved for values
            Err(ErrorCode::InvalidOperand)?;
        }
        let stack_index_labels_a = self.get_stack_index(pos_labels_a)?;
        let stack_index_labels_b = self.get_stack_index(pos_labels_b)?;
        let (v1, rest) = self
            .stack
            .split_last_mut()
            .ok_or_else(|| ErrorCode::StackUnderflow)?;

        let labels_a = &rest[stack_index_labels_a];
        let labels_b = &rest[stack_index_labels_b];

        match (v1, labels_a, labels_b) {
            (
                Operand::Vector(v1),
                Operand::Labels(labels_a),
                Operand::Labels(labels_b),
            ) => {
                if v1.data.len() != labels_a.data.len() {
                    Err(ErrorCode::InvalidOperand)?;
                }
                let mut j = 0;
                let mut k = 0;
                for i in 0..labels_a.data.len() {
                    let label_a = labels_a.data[i];
                    while j < labels_b.data.len() {
                        let label_b = labels_b.data[j];
                        if label_b < label_a {
                            // Label B not in A. B must be a subset of A.
                            Err(ErrorCode::NotFound)?;
                        } else if label_a < label_b {
                            // Label A not in B.
                            v1.data.remove(k);
                            break;
                        } else {
                            // Found matching label in B.
                            j += 1;
                            k += 1;
                            // go to next A and next B
                            break;
                        }
                    }
                }
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }

        Ok(())
    }
}

#[cfg(test)]
fn log_stack_fun(stack: &Stack) {
    log_msg!("\n[REGISTRY]");
    for i in 0..stack.registry.len() {
        log_msg!(
            "[{}] {}",
            i,
            match &stack.registry[i] {
                Operand::None => format!("None"),
                Operand::Labels(labels) => format!("Labels: {}", *labels),
                Operand::Vector(vector) => format!("Vector: {:0.5}", *vector),
                Operand::Scalar(amount) => format!("Scalar: {:0.5}", *amount),
                Operand::Label(label) => format!("Label: {}", label),
            }
        );
    }

    log_msg!("\n[STACK]");
    for i in 0..stack.stack.len() {
        log_msg!(
            "[{}] {}",
            i,
            match &stack.stack[i] {
                Operand::None => format!("None"),
                Operand::Labels(labels) => format!("Labels: {}", *labels),
                Operand::Vector(vector) => format!("Vector: {:0.5}", *vector),
                Operand::Scalar(amount) => format!("Scalar: {:0.5}", *amount),
                Operand::Label(label) => format!("Label: {}", label),
            }
        );
    }

    log_msg!("---");
}

#[cfg(not(test))]
#[macro_export]
macro_rules! log_stack {
    ($($t:tt)*) => {};
}

#[cfg(test)]
#[macro_export]
macro_rules! log_stack {
    ($arg:expr) => {
        $crate::program::log_stack_fun($arg);
    };
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
        log_msg!("\nvvv EXECUTE PROGRAM vvv");
        log_stack!(&stack);

        let mut pc = 0;
        while pc < code.len() {
            let op_code = code[pc];
            log_msg!("PC = {:4}, OpCode = {:4}", pc, op_code);
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
                OP_T => {
                    let count = code[pc] as usize;
                    pc += 1;
                    stack.transpose(count)?;
                }
                OP_ADD => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.add(pos)?;
                }
                OP_SUB => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.sub(pos)?;
                }
                OP_SSB => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.ssb(pos)?;
                }
                OP_MUL => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.mul(pos)?;
                }
                OP_DIV => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.div(pos)?;
                }
                OP_SQRT => {
                    stack.sqrt()?;
                }
                OP_VSUM => {
                    stack.vsum()?;
                }
                OP_MIN => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.min(pos)?;
                }
                OP_MAX => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.max(pos)?;
                }
                OP_LUNION => {
                    let pos = code[pc] as usize;
                    pc += 1;
                    stack.lunion(pos)?;
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
                OP_VMIN => {
                    stack.vmin()?;
                }
                OP_VMAX => {
                    stack.vmax()?;
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
                OP_POPN => {
                    let count = code[pc] as usize;
                    pc += 1;
                    stack.op_popn(count)?;
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
                OP_JSBB => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    stack.jssb(pos_1, pos_2)?;
                }
                OP_JXPND => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    stack.jxpnd(pos_1, pos_2)?;
                }
                OP_JFLTR => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    stack.jfltr(pos_1, pos_2)?;
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
                    let res = prg.execute_with_stack(cod.data, &mut st);
                    if let Err(err) = res {
                        log_msg!("\n\nError occurred in procedure:");
                        log_stack!(&st);
                        log_msg!("^^^ Stack of the procedure\n\n");
                        return Err(err);
                    }
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

        log_stack!(&stack);
        log_msg!("\n^^^ PROGRAM ENDED ^^^");
        Ok(())
    }
}

#[cfg(test)]
pub mod test {
    use std::collections::HashMap;

    use alloy_primitives::U128;
    use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector};

    use super::ErrorCode;
    use super::*; // Use glob import for tidiness

    struct TestVectorIO {
        labels: HashMap<U128, Labels>,
        vectors: HashMap<U128, Vector>,
        scalars: HashMap<U128, Amount>,
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
        fn load_labels(&self, id: U128) -> Result<Labels, ErrorCode> {
            let v = self.labels.get(&id).ok_or_else(|| ErrorCode::NotFound)?;
            Ok(Labels {
                data: v.data.clone(),
            })
        }

        fn load_vector(&self, id: U128) -> Result<Vector, ErrorCode> {
            let v = self.vectors.get(&id).ok_or_else(|| ErrorCode::NotFound)?;
            Ok(Vector {
                data: v.data.clone(),
            })
        }

        fn load_scalar(&self, id: U128) -> Result<Amount, ErrorCode> {
            let v = self.scalars.get(&id).ok_or_else(|| ErrorCode::NotFound)?;
            Ok(*v)
        }

        fn store_labels(&mut self, id: U128, input: Labels) -> Result<(), ErrorCode> {
            self.labels.insert(id, input);
            Ok(())
        }

        fn store_vector(&mut self, id: U128, input: Vector) -> Result<(), ErrorCode> {
            self.vectors.insert(id, input);
            Ok(())
        }

        fn store_scalar(&mut self, id: U128, input: Amount) -> Result<(), ErrorCode> {
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
        let assets_id = U128::from(101);
        let weights_id = U128::from(102);
        let quote_id = U128::from(201);
        let order_id = U128::from(301);
        let order_quantities_id = U128::from(401);
        let solve_quadratic_id = U128::from(901);

        vio.store_labels(
            assets_id,
            Labels {
                data: vec![1001, 1002, 1003],
            },
        )
        .unwrap();

        vio.store_vector(
            weights_id,
            Vector {
                data: vec![
                    Amount::from_u128_with_scale(0_100, 3),
                    Amount::from_u128_with_scale(1_000, 3),
                    Amount::from_u128_with_scale(100_0, 1),
                ],
            },
        )
        .unwrap();

        vio.store_vector(
            quote_id,
            Vector {
                data: vec![
                    Amount::from_u128_with_scale(10_00, 2),
                    Amount::from_u128_with_scale(10_000, 0),
                    Amount::from_u128_with_scale(100_0, 1),
                ],
            },
        )
        .unwrap();

        vio.store_vector(
            order_id,
            Vector {
                data: vec![
                    Amount::from_u128_with_scale(1000_00, 2),
                    Amount::from_u128_with_scale(0, 0),
                    Amount::from_u128_with_scale(0, 0),
                ],
            },
        )
        .unwrap();

        #[rustfmt::skip]
        let solve_quadratic_vectorized = vec![
            // 1. Initial Load and Setup (assuming stack starts with [C_vec, P_vec, S_vec])
            OP_STR, 1, // S_vec -> R1, POP S_vec
            OP_STR, 2, // P_vec -> R2, POP P_vec
            OP_STR, 3, // C_vec -> R3, POP C_vec

            // 2. Compute P^2 (R4)
            OP_LDR, 2, 
            OP_MUL, 0, // P^2 = P * P (Vector self-multiplication)
            OP_STR, 4, // P^2 -> R4, POP P^2

            // 3. Compute Radical (R5)
            OP_LDR, 1, OP_LDR, 3, OP_MUL, 1, // [S, SC] (Vector * Vector)
            OP_IMMS, Amount::FOUR.to_u128_raw(), OP_MUL, 1, // [S, SC, 4SC] (Vector * Scalar)
            OP_LDR, 4, // [S, SC, 4SC, P^2]
            OP_ADD, 1, // [S, SC, 4SC, P^2+4SC] (Vector + Vector)
            OP_SQRT, // [S, SC, 4SC, R] (Vector square root)
            OP_STR, 5, // R -> R5, POP R

            // 4. Compute Numerator: R - min(R, P)
            OP_LDR, 5, OP_LDR, 2, // [..., R, P]
            OP_MIN, 1, // [..., R, min(R, P)] (Vector pairwise MIN)
            OP_SWAP, 1, // [..., min(R, P), R] 
            OP_SUB, 1, // [..., min(R, P), N] (Vector - Vector subtraction)
            
            // 5. Compute X = Num / 2S
            OP_LDR, 1, OP_IMMS, Amount::TWO.to_u128_raw(), // [..., min, N, S, 2]
            OP_SWAP, 1, // [..., min, N, 2, S]
            OP_MUL, 1, // [..., min, N, 2, 2S] (Vector * Scalar multiplication)
            
            OP_SWAP, 2, // [..., min, 2S, 2, N] (N at pos 0, 2S at pos 2)
            OP_DIV, 2, // [..., min, 2S, 2, X]. X = N / 2S. (Vector / Vector division)
            // Final Vector X is at the top of the stack.
        ];

        vio.store_labels(
            solve_quadratic_id,
            Labels {
                data: solve_quadratic_vectorized,
            },
        )
        .unwrap();

        let reg_weights = 0;
        let reg_collateral = 1;
        let reg_capacity = 2;
        let reg_price = 3;
        let reg_slope = 4;

        #[rustfmt::skip]
        let code = vec![
            OP_LDV, weights_id.to::<u128>(), // Stack: [AW]
            OP_STR, reg_weights,             // Stack: []
            
            // Extract Collateral (Order vector: [Collateral, Spent, Minted])
            OP_LDV, order_id.to::<u128>(),   // Stack: [O]
            OP_UNPK,                         // Stack: [Minted, Spent, Collateral]
            OP_POPN, 2,                       // Stack: [Collateral]
            OP_STR, reg_collateral,          // Stack: []

            // Extract Price and Slope (Quote vector: [Capacity, Price, Slope])
            OP_LDV, quote_id.to::<u128>(),   // Stack: [Q]
            OP_UNPK,                         // Stack: [Slope, Price, Capacity]
            OP_POPN, 1,                       // Stack: [Slope, Price] (Capacity discarded)

            // Stack is now [Slope, Price]. Load Collateral to get arguments in order.
            OP_LDR, reg_collateral,          // Stack: [Slope, Price, Collateral]
            
            // Call procedure: Inputs are Collateral (TOS), Price, Slope.
            OP_B, solve_quadratic_id.to::<u128>(), 3, 1, 8, // Stack: [IndexQuantity]

            // Apply Weights and Store Result
            OP_LDR, reg_weights,             // Stack: [IQ, AW]
            OP_MUL, 1,                       // Stack: [AssetQuantities]
            OP_STV, order_quantities_id.to::<u128>(), // Stack: []
        ];

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

        assert_eq!(order.data, vec![
            Amount::from_u128_with_scale(1000, 0),
            Amount::from_u128_with_scale(0, 0),
            Amount::from_u128_with_scale(0, 0),
        ]);
        
        assert_eq!(quote.data, vec![
            Amount::from_u128_with_scale(10, 0),
            Amount::from_u128_with_scale(10_000, 0),
            Amount::from_u128_with_scale(100, 0),
        ]);
        
        assert_eq!(weigths.data, vec![
            Amount::from_u128_with_scale(1, 1),
            Amount::from_u128_with_scale(1, 0),
            Amount::from_u128_with_scale(100, 0),
        ]);
        
        // these are exact expected fixed point decimal values as raw u128
        assert_eq!(order_quantites.data, vec![
            Amount(99990001950000), Amount(999900019500000), Amount(99990001950000000)
        ]);
    }

    #[test]
    fn test_transpose() {
        let mut vio = TestVectorIO::new();
        let num_registers = 8;

        // Utility to create a readable Amount (e.g., 5 is "5.0")
        let a = |x: u128| Amount::from_u128_with_scale(x, 0);

        // --- 1. Setup VIO Inputs ---
        let vector1_id = U128::from(100);
        let vector2_id = U128::from(101);
        let expected1_id = U128::from(102); // T1: [1, 4]
        let expected2_id = U128::from(103); // T2: [2, 5]
        let expected3_id = U128::from(104); // T3: [3, 6]
        let delta_id = U128::from(105);

        // V1: [1.0, 2.0, 3.0]
        let v1 = Vector {
            data: vec![a(1), a(2), a(3)],
        };
        // V2: [4.0, 5.0, 6.0]
        let v2 = Vector {
            data: vec![a(4), a(5), a(6)],
        };

        // Expected Transposed Columns (T1, T2, T3)
        let t1_expected = Vector {
            data: vec![a(1), a(4)],
        };
        let t2_expected = Vector {
            data: vec![a(2), a(5)],
        };
        let t3_expected = Vector {
            data: vec![a(3), a(6)],
        };

        vio.store_vector(vector1_id, v1).unwrap();
        vio.store_vector(vector2_id, v2).unwrap();
        vio.store_vector(expected1_id, t1_expected).unwrap();
        vio.store_vector(expected2_id, t2_expected).unwrap();
        vio.store_vector(expected3_id, t3_expected).unwrap();

        // --- 2. VIL Code Execution ---
        #[rustfmt::skip]
        let code = vec![
            // 1. Setup Transposition
            OP_LDV, vector1_id.to::<u128>(), // Stack: [V1]
            OP_LDV, vector2_id.to::<u128>(), // Stack: [V1, V2]
            OP_T, 2,                         // Stack: [T1, T2, T3] (3 vectors)

            // 2. Load Expected Vectors for comparison
            OP_LDV, expected1_id.to::<u128>(), // [T1, T2, T3, E1]
            OP_LDV, expected2_id.to::<u128>(), // [T1, T2, T3, E1, E2]
            OP_LDV, expected3_id.to::<u128>(), // [T1, T2, T3, E1, E2, E3] (6 vectors)

            // 3. D3 = T3 - E3
            OP_SUB, 3,                         // Stack: [T1, T2, T3, E1, E2, D3]
            
            // 4. D2 = T2 - E2
            OP_SWAP, 1,                        // Stack: [T1, T2, T3, E1, D3, E2]
            OP_SUB, 4,                         // Stack: [T1, T2, T3, E1, D3, D2]

            // 5. D1 = T1 - E1
            OP_SWAP, 2,                        // Stack: [T1, T2, T3, D2, D3, E1]
            OP_SUB, 5,                         // Stack: [T1, T2, T3, D2, D3, D1]

            // 6. Compute total delta - should be zero
            OP_ADD, 1,                         // Stack: [T1, T2, T3, D2, D3, D1 + D3]
            OP_ADD, 2,                         // Stack: [T1, T2, T3, D2, D3, D1 + D3 + D2]
            
            // 7. Store the final zero vector
            OP_STV, delta_id.to::<u128>(),
        ];

        let mut stack = Stack::new(num_registers);
        let mut program = Program::new(&mut vio);

        if let Err(err) = program.execute_with_stack(code, &mut stack) {
            log_stack!(&stack);
            panic!("Failed to execute test: {:?}", err);
        }

        // --- 3. Assertion ---
        let delta = vio.load_vector(delta_id).unwrap();

        assert_eq!(delta.data, vec![Amount::ZERO; 2]);
    }
}
