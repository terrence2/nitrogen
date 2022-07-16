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
use crate::{
    memory::{CallResult, ComponentLookup, ResourceLookup, RustCallbackFunc, WorldIndex},
    HeapMut, HeapRef, ScriptComponent, ScriptResource,
};
use anyhow::{anyhow, bail, Result};
use bevy_ecs::{prelude::*, system::Resource};
use futures::Future;
use geodesy::{GeoSurface, Graticule, Target};
use itertools::Itertools;
use log::error;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
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
    Resource(ResourceLookup),
    ResourceMethod(ResourceLookup, String), // TODO: atoms?
    Entity(Entity),
    Component(Entity, ComponentLookup),
    ComponentMethod(Entity, ComponentLookup, String), // TODO: atoms?
    RustMethod(Arc<RustCallbackFunc>),
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

    pub(crate) fn new_resource(lookup: &ResourceLookup) -> Self {
        Self::Resource(lookup.to_owned())
    }

    pub(crate) fn new_entity(entity: Entity) -> Self {
        Self::Entity(entity)
    }

    pub(crate) fn new_component(entity: Entity, lookup: &ComponentLookup) -> Self {
        Value::Component(entity, lookup.to_owned())
    }

    pub fn from_bool(v: bool) -> Self {
        Self::Boolean(v)
    }

    pub fn from_int(v: i64) -> Self {
        Self::Integer(v)
    }

    pub fn from_float(v: f64) -> Self {
        Self::Float(OrderedFloat(v))
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str<S: ToString>(v: S) -> Self {
        Self::String(v.to_string())
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

    pub fn make_resource_method<T>(name: &str) -> Self
    where
        T: Resource + ScriptResource + 'static,
    {
        Self::ResourceMethod(ResourceLookup::new::<T>(), name.to_owned())
    }

    pub fn make_component_method<T>(entity: Entity, name: &str) -> Self
    where
        T: Component + ScriptComponent + 'static,
    {
        Self::ComponentMethod(entity, ComponentLookup::new::<T>(), name.to_owned())
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

    pub fn attr(&self, name: &str, heap: HeapRef) -> Result<Value> {
        match self {
            Value::Resource(lookup) => lookup
                .get_ref(heap.world())
                .ok_or_else(|| anyhow!("no such resource for attr: {}", name))?
                .get(name),
            Value::Entity(entity) => {
                // TODO: there's almost certainly a smarter way to do this.
                if name == "list" {
                    #[allow(unstable_name_collisions)]
                    let msg: Value = heap
                        .entity_component_names(*entity)
                        .map(|v| v.intersperse("\n").collect())
                        .unwrap_or_else(|| "error: unknown entity name".to_owned())
                        .into();
                    Ok(Value::RustMethod(Arc::new(move |_, _| Ok(msg.clone()))))
                } else {
                    heap.maybe_component_value_by_name(*entity, name)
                        .ok_or_else(|| anyhow!("no such component {} on entity {:?}", name, entity))
                }
            }
            Value::Component(entity, lookup) => lookup
                .get_ref(*entity, heap.world())
                .ok_or_else(|| anyhow!("no such component for attr: {}", name))?
                .get(*entity, name),
            _ => bail!(
                "attribute base must be a resource, entity, or component, not {:?}",
                self
            ),
        }
    }

    pub fn store_attr(&mut self, name: &str, value: Value, mut heap: HeapMut) -> Result<()> {
        match self {
            Value::Resource(lookup) => lookup
                .get_mut(heap.world_mut())
                .ok_or_else(|| anyhow!("no such resource for attr: {}", name))?
                .put(name, value),
            Value::Component(entity, lookup) => lookup
                .get_mut(*entity, heap.world_mut())
                .ok_or_else(|| anyhow!("no such component for attr: {}", name))?
                .put(*entity, name, value),
            _ => bail!(
                "attribute base must be a resource, entity, or component, not {:?}",
                self
            ),
        }
    }

    pub fn call_method(&mut self, args: &[Value], heap: HeapMut) -> Result<Value> {
        match self {
            Value::ResourceMethod(lookup, method_name) => {
                Ok(match lookup.call_method(method_name, args, heap)? {
                    CallResult::Val(v) => v,
                    CallResult::Selfish => Value::Resource(lookup.to_owned()),
                })
            }
            Value::ComponentMethod(entity, lookup, method_name) => Ok(
                match lookup.call_method(*entity, method_name, args, heap)? {
                    CallResult::Val(v) => v,
                    CallResult::Selfish => Value::Component(*entity, lookup.to_owned()),
                },
            ),
            Value::RustMethod(method) => method(args, heap),
            _ => {
                error!("attempting to call non-method value: {}", self);
                bail!("attempting to call non-method value: {}", self);
            }
        }
    }

    pub fn attrs<'a>(
        &'a self,
        index: &'a WorldIndex,
        world: &'a mut World,
    ) -> Result<Vec<&'a str>> {
        Ok(match self {
            Value::Resource(lookup) => lookup
                .get_ref(world)
                .ok_or_else(|| anyhow!("no such resource for names"))?
                .names(),
            Value::Entity(entity) => index.component_attrs(entity),
            Value::Component(entity, lookup) => lookup
                .get_ref(*entity, world)
                .ok_or_else(|| anyhow!("no such component for attrs"))?
                .names(),
            _ => bail!(
                "attribute base must be a resource, entity, or component, not {:?}",
                self
            ),
        })
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
            Self::Resource(_) => write!(f, "<resource>"),
            Self::ResourceMethod(_, name) => {
                write!(f, "<resource>.{}", name)
            }
            Self::Entity(ent) => write!(f, "@{:?}", ent),
            Self::Component(ent, _) => write!(f, "@[{:?}].<lookup>", ent),
            Self::ComponentMethod(ent, _, name) => write!(f, "@[{:?}].<lookup>.{}", ent, name),
            Self::RustMethod(_) => write!(f, "<callback>"),
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
            Self::Entity(a) => match other {
                Self::Entity(b) => a == b,
                _ => false,
            },
            Self::Resource(_) => false,
            Self::Component(_, _) => false,
            Self::ComponentMethod(_, _, _) => false,
            Self::ResourceMethod(_, _) => false,
            Self::RustMethod(_) => false,
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
