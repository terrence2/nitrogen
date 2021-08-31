// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use crate::Module;
use anyhow::{bail, Result};
use futures::Future;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::fmt::Formatter;
use std::{fmt, fmt::Debug, pin::Pin, sync::Arc};

#[derive(Clone)]
pub enum Value {
    Boolean(bool),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
    Module(Arc<RwLock<dyn Module>>),
    Method(Arc<RwLock<dyn Module>>, String), // TODO: atoms
    Future(Arc<RwLock<Pin<Box<dyn Future<Output = Value> + Send + Sync + Unpin + 'static>>>>),
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Value {
    #[allow(non_snake_case)]
    pub fn True() -> Self {
        Self::Boolean(true)
    }

    #[allow(non_snake_case)]
    pub fn False() -> Self {
        Self::Boolean(false)
    }

    pub fn to_bool(&self) -> Result<bool> {
        if let Self::Boolean(b) = self {
            return Ok(*b);
        }
        bail!("not a boolean value: {}", self)
    }

    pub fn to_int(&self) -> Result<i64> {
        if let Self::Integer(i) = self {
            return Ok(*i);
        }
        bail!("not an integer value: {}", self)
    }

    pub fn to_float(&self) -> Result<f64> {
        if let Self::Float(f) = self {
            return Ok(f.0);
        }
        bail!("not a float value: {}", self)
    }

    pub fn to_future(
        &self,
    ) -> Result<Arc<RwLock<Pin<Box<(dyn Future<Output = Value> + Send + Sync + Unpin + 'static)>>>>>
    {
        if let Self::Future(f) = self {
            return Ok(f.clone());
        }
        bail!("not a future value: {}", self)
    }

    pub fn to_str(&self) -> Result<&str> {
        if let Self::String(s) = self {
            return Ok(s);
        }
        bail!("not a string value: {}", self)
    }

    pub fn to_method(&self) -> Result<(Arc<RwLock<dyn Module>>, &str)> {
        if let Self::Method(module, name) = self {
            return Ok((module.clone(), name));
        }
        bail!("not a method value: {}", self)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::Float(OrderedFloat(f))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(v) => write!(f, "{}", v),
            Self::Integer(v) => write!(f, "{}", v),
            Self::Float(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "\"{}\"", v),
            Self::Module(v) => write!(f, "{}", v.read().module_name()),
            Self::Method(v, name) => write!(f, "{}.{}", v.read().module_name(), name),
            Self::Future(_) => write!(f, "Future"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Boolean(a) => match other {
                Self::Boolean(b) => a == b,
                _ => false,
            },
            Self::Integer(a) => match other {
                Self::Integer(b) => a == b,
                _ => false,
            },
            Self::Float(a) => match other {
                Self::Float(b) => a == b,
                _ => false,
            },
            Self::String(a) => match other {
                Self::String(b) => a == b,
                _ => false,
            },
            Self::Module(_) => false,
            Self::Method(_, _) => false,
            Self::Future(_) => false,
        }
    }
}

impl Eq for Value {}

impl Value {
    pub fn impl_multiply(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs * rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) * rhs),
                Value::String(_) => bail!("cannot multiply a number by a string"),
                Value::Boolean(_) => bail!("cannot multiply a number to a boolean"),
                Value::Module(_) => bail!("cannot multiply a number by a module"),
                Value::Method(_, _) => bail!("cannot multiply a number by a method"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs * OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs * rhs),
                Value::String(_) => bail!("cannot multiply a number by a string"),
                Value::Boolean(_) => bail!("cannot multiply a number to a boolean"),
                Value::Module(_) => bail!("cannot multiply a number by a module"),
                Value::Method(_, _) => bail!("cannot multiply a number by a method"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::String(lhs) => match other {
                Value::Integer(rhs) => Value::String(lhs.repeat(rhs.max(0) as usize)),
                Value::Float(rhs) => Value::String(lhs.repeat(rhs.floor().max(0f64) as usize)),
                Value::String(_) => bail!("cannot multiply a string by a string"),
                Value::Boolean(_) => bail!("cannot multiply a number to a boolean"),
                Value::Module(_) => bail!("cannot multiply a string by a module"),
                Value::Method(_, _) => bail!("cannot multiply a string by a method"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
            Value::Future(_) => bail!("cannot do arithmetic on a future"),
        })
    }

    pub fn impl_divide(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs / rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) / rhs),
                Value::String(_) => bail!("cannot divide a number by a string"),
                Value::Boolean(_) => bail!("cannot divide a number by a boolean"),
                Value::Module(_) => bail!("cannot divide a number by a module"),
                Value::Method(_, _) => bail!("cannot divide a number by a method"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs / OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs / rhs),
                Value::String(_) => bail!("cannot divide a number by a string"),
                Value::Boolean(_) => bail!("cannot divide a number by a boolean"),
                Value::Module(_) => bail!("cannot divide a number by a module"),
                Value::Method(_, _) => bail!("cannot divide a number by a method"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::String(_) => bail!("cannot divide a string by anything"),
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
            Value::Future(_) => bail!("cannot do arithmetic on a future"),
        })
    }

    pub fn impl_add(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs + rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) + rhs),
                Value::String(_) => bail!("cannot add a string to a number"),
                Value::Boolean(_) => bail!("cannot add a number to a boolean"),
                Value::Module(_) => bail!("cannot add a module to a number"),
                Value::Method(_, _) => bail!("cannot add a method to a number"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs + OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs + rhs),
                Value::String(_) => bail!("cannot add a string to a number"),
                Value::Boolean(_) => bail!("cannot add a number to a boolean"),
                Value::Module(_) => bail!("cannot add a module to a number"),
                Value::Method(_, _) => bail!("cannot add a method to a number"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::String(lhs) => match other {
                Value::Integer(_) => bail!("cannot add a number to a string"),
                Value::Float(_) => bail!("cannot add a number to a string"),
                Value::String(rhs) => Value::String(lhs + &rhs),
                Value::Boolean(_) => bail!("cannot add a number to a boolean"),
                Value::Module(_) => bail!("cannot add a module to a string"),
                Value::Method(_, _) => bail!("cannot add a method to a string"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
            Value::Future(_) => bail!("cannot do arithmetic on a future"),
        })
    }

    pub fn impl_subtract(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs - rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) - rhs),
                Value::String(_) => bail!("cannot subtract a string from a number"),
                Value::Boolean(_) => bail!("cannot subtract a boolean from a number"),
                Value::Module(_) => bail!("cannot subtract a module from a number"),
                Value::Method(_, _) => bail!("cannot subtract a method from a number"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs - OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs - rhs),
                Value::String(_) => bail!("cannot subtract a string from a number"),
                Value::Boolean(_) => bail!("cannot subtract a boolean from a number"),
                Value::Module(_) => bail!("cannot subtract a module from a number"),
                Value::Method(_, _) => bail!("cannot subtract a method from a number"),
                Value::Future(_) => bail!("cannot do arithmetic on a future"),
            },
            Value::String(_) => bail!("cannot subtract with a string"),
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
            Value::Future(_) => bail!("cannot do arithmetic on a future"),
        })
    }
}
