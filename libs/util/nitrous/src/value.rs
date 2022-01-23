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
use geodesy::{GeoSurface, Graticule, Target};
use log::error;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::{
    fmt::{self, Debug, Formatter},
    pin::Pin,
    sync::Arc,
};

pub type FutureValue = Pin<Box<dyn Future<Output = Value> + Send + Sync + Unpin + 'static>>;

#[derive(Clone)]
pub enum Value {
    Boolean(bool),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
    Graticule(Graticule<GeoSurface>),
    Module(Arc<RwLock<dyn Module>>),
    Method(Arc<RwLock<dyn Module>>, String), // TODO: atoms
    Future(Arc<RwLock<FutureValue>>),
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Future(_) => write!(f, "FutureValue"),
            _ => write!(f, "{}", self),
        }
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

    pub fn is_graticule(&self) -> bool {
        matches!(self, Self::Graticule(_))
    }

    pub fn to_grat_surface(&self) -> Result<Graticule<GeoSurface>> {
        if let Self::Graticule(grat) = self {
            return Ok(*grat);
        }
        bail!("not a graticule value: {}", self)
    }

    pub fn to_grat_target(&self) -> Result<Graticule<Target>> {
        if let Self::Graticule(grat) = self {
            return Ok(grat.with_origin::<Target>());
        }
        bail!("not a graticule value: {}", self)
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

    pub fn to_future(&self) -> Result<Arc<RwLock<FutureValue>>> {
        if let Self::Future(f) = self {
            return Ok(f.clone());
        }
        bail!("not a future value: {}", self)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Float(_) | Self::Integer(_))
    }

    pub fn to_numeric(&self) -> Result<f64> {
        Ok(match self {
            Self::Float(f) => f.0,
            Self::Integer(i) => *i as f64,
            _ => bail!("not numeric"),
        })
    }

    pub fn spawn_method(&self, args: &[Value]) {
        fn inner(callable: Value, args: SmallVec<[Value; 2]>) -> Result<()> {
            let (module, name) = callable.to_method()?;
            module.write().call_method(name, &args)?;
            Ok(())
        }
        let callable = self.to_owned();
        let args = SmallVec::from(args);
        rayon::spawn(move || {
            if let Err(e) = inner(callable, args) {
                error!("spawn_method failed with: {}", e);
            }
        });
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::Float(OrderedFloat(f))
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::String(s.to_owned())
    }
}

impl From<Graticule<GeoSurface>> for Value {
    fn from(grat: Graticule<GeoSurface>) -> Self {
        Self::Graticule(grat)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(v) => write!(f, "{}", v),
            Self::Integer(v) => write!(f, "{}", v),
            Self::Float(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "\"{}\"", v),
            Self::Graticule(v) => write!(f, "{}", v),
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
            Self::Graticule(a) => match other {
                Self::Graticule(b) => a == b,
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
                _ => bail!("invalid rhs type for multiply with integer"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs * OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs * rhs),
                _ => bail!("invalid rhs type for multiply with float"),
            },
            Value::String(lhs) => match other {
                Value::Integer(rhs) => Value::String(lhs.repeat(rhs.max(0) as usize)),
                Value::Float(rhs) => Value::String(lhs.repeat(rhs.floor().max(0f64) as usize)),
                _ => bail!("invalid rhs type for multiply with string"),
            },
            _ => bail!("cannot do arithmetic with this type of value"),
        })
    }

    pub fn impl_divide(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs / rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) / rhs),
                _ => bail!("invalid rhs type for divide from integer"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs / OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs / rhs),
                _ => bail!("invalid rhs type for divide from float"),
            },
            _ => bail!("cannot divide from this type of value"),
        })
    }

    pub fn impl_add(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs + rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) + rhs),
                _ => bail!("invalid rhs type for add to integer"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs + OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs + rhs),
                _ => bail!("invalid rhs type for add to float"),
            },
            Value::String(lhs) => match other {
                Value::String(rhs) => Value::String(lhs + &rhs),
                _ => bail!("invalid rhs type for add to string"),
            },
            _ => bail!("cannot add to this type of value"),
        })
    }

    pub fn impl_subtract(self, other: Self) -> Result<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs - rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) - rhs),
                _ => bail!("invalid rhs type for subtract from integer"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs - OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs - rhs),
                _ => bail!("invalid rhs type for subtract from float"),
            },
            _ => bail!("cannot subtract from this type of value"),
        })
    }
}
