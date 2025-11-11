use core::mem::swap;

#[cfg(test)]
use core::fmt::Debug;

use alloc::vec::Vec;
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
    fn load_labels(&self, id: u128) -> Result<Labels, ErrorCode>;
    fn load_vector(&self, id: u128) -> Result<Vector, ErrorCode>;

    fn store_labels(&mut self, id: u128, input: Labels) -> Result<(), ErrorCode>;
    fn store_vector(&mut self, id: u128, input: Vector) -> Result<(), ErrorCode>;
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

pub(crate) struct Stack {
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
    pub(crate) fn new(num_registers: usize) -> Self {
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

    fn ldm(&mut self, pos: usize) -> Result<(), ErrorCode> {
        let v1 = self
            .registry
            .get_mut(pos)
            .ok_or_else(|| ErrorCode::OutOfRange)?;
        let mut v2 = Operand::None;
        swap(v1, &mut v2);
        self.push(v2);
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

        let v2 = rest.get(stack_index).ok_or_else(|| ErrorCode::OutOfRange)?;

        match (v1, v2) {
            (Operand::Labels(labels_a), Operand::Labels(labels_b)) => {
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

    fn jupd(
        &mut self,
        pos_vector_b: usize,
        pos_labels_a: usize,
        pos_labels_b: usize,
    ) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then it's just normal vector add
            return self.add(1);
        }
        if pos_labels_a < 2 || pos_labels_b < 2 {
            // [TOS - 1, TOS] are reserved for values
            Err(ErrorCode::InvalidOperand)?;
        }
        let stack_index_v2 = self.get_stack_index(pos_vector_b)?;
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
                            v1.data[i] = v2.data[j];
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

    fn jadd(
        &mut self,
        pos_vector_b: usize,
        pos_labels_a: usize,
        pos_labels_b: usize,
    ) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then it's just normal vector add
            return self.add(1);
        }
        if pos_labels_a < 2 || pos_labels_b < 2 {
            // [TOS - 1, TOS] are reserved for values
            Err(ErrorCode::InvalidOperand)?;
        }
        let stack_index_v2 = self.get_stack_index(pos_vector_b)?;
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

    fn jflt(&mut self, pos_labels_a: usize, pos_labels_b: usize) -> Result<(), ErrorCode> {
        if pos_labels_a == pos_labels_b {
            // If both vectors use same labels, then no work needed
            return Ok(());
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
            (Operand::Vector(v1), Operand::Labels(labels_a), Operand::Labels(labels_b)) => {
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
                // Drop any further A as their labels didn't match any labels in B
                drop(v1.data.drain(k..))
            }
            _ => Err(ErrorCode::InvalidOperand)?,
        }

        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn log_stack_fun(stack: &Stack) {
    log_msg!("\n[REGISTRY]");
    for _i in 0..stack.registry.len() {
        log_msg!(
            "[{}] {}",
            _i,
            match &stack.registry[_i] {
                Operand::None => format!("None"),
                Operand::Labels(labels) => format!("Labels: {}", *labels),
                Operand::Vector(vector) => format!("Vector: {:0.5}", *vector),
                Operand::Scalar(amount) => format!("Scalar: {:0.5}", *amount),
                Operand::Label(label) => format!("Label: {}", label),
            }
        );
    }

    log_msg!("\n[STACK]");
    for _i in 0..stack.stack.len() {
        log_msg!(
            "[{}] {}",
            _i,
            match &stack.stack[_i] {
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

    pub(crate) fn execute_with_stack(
        &mut self,
        code: Vec<u128>,
        stack: &mut Stack,
    ) -> Result<(), ErrorCode> {
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
                    let v = self.vio.load_labels(id)?;
                    stack.push(Operand::Labels(v));
                }
                OP_LDV => {
                    let id = code[pc];
                    pc += 1;
                    let v = self.vio.load_vector(id)?;
                    stack.push(Operand::Vector(v));
                }
                OP_STL => {
                    let id = code[pc];
                    pc += 1;
                    match stack.pop()? {
                        Operand::Labels(v) => {
                            self.vio.store_labels(id, v)?;
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
                            self.vio.store_vector(id, v)?;
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
                OP_LDM => {
                    let reg = code[pc] as usize;
                    pc += 1;
                    stack.ldm(reg)?;
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
                OP_JUPD => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    let pos_3 = code[pc] as usize;
                    pc += 1;
                    stack.jupd(pos_1, pos_2, pos_3)?;
                }
                OP_JADD => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    let pos_3 = code[pc] as usize;
                    pc += 1;
                    stack.jadd(pos_1, pos_2, pos_3)?;
                }
                OP_JFLT => {
                    let pos_1 = code[pc] as usize;
                    pc += 1;
                    let pos_2 = code[pc] as usize;
                    pc += 1;
                    stack.jflt(pos_1, pos_2)?;
                }
                OP_B => {
                    // B <program_id> <num_inputs> <num_outputs> <num_registers>
                    let code_address = code[pc];
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
                    let code_address = code[pc];
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
