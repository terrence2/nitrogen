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
    heap::{HeapMut, HeapRef},
    value::Value,
};
use anyhow::{anyhow, ensure, Result};
use bevy_ecs::{prelude::*, system::Resource};
use std::borrow::BorrowMut;
use std::{collections::HashMap, fmt::Debug, sync::Arc};

/// Use #[derive(NitrousResource)] to implement this trait. The derived implementation
/// will expect the struct to have an impl block annotated with #[inject_nitrous]. This
/// second macro will use #[method] tags to populate lookups for the various operations.
pub trait ScriptResource: 'static {
    fn resource_type_name(&self) -> String;
    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value>;
    fn put(&mut self, name: &str, value: Value) -> Result<()>;
    fn get(&self, name: &str) -> Result<Value>;
    fn names(&self) -> Vec<&str>;
}

/// Bridges from a name (as in a script) to ScriptResouce. Effectively it stores the T
/// for us so that we don't have to do TypeId and pointer hyjinx.
pub type ResourceLookupRefFunc =
    dyn Fn(&World) -> Option<&(dyn ScriptResource + 'static)> + Send + Sync + 'static;
pub type ResourceLookupMutFunc =
    dyn Fn(&mut World) -> Option<&mut (dyn ScriptResource + 'static)> + Send + Sync + 'static;

#[derive(Clone)]
pub struct ResourceLookup {
    ref_func: Arc<ResourceLookupRefFunc>,
    mut_func: Arc<ResourceLookupMutFunc>,
}

impl ResourceLookup {
    pub fn new<T>() -> Self
    where
        T: Resource + ScriptResource + 'static,
    {
        Self {
            ref_func: Arc::new(move |world| {
                world.get_resource::<T>().map(|ptr| {
                    let rto: &(dyn ScriptResource + 'static) = ptr;
                    rto
                })
            }),
            mut_func: Arc::new(move |world| {
                world.get_resource_mut::<T>().map(|ptr| {
                    let rto: &mut (dyn ScriptResource + 'static) = ptr.into_inner();
                    rto
                })
            }),
        }
    }

    pub fn as_ref<'a>(&self, world: &'a World) -> &'a dyn ScriptResource {
        (self.ref_func)(world).expect("resource present")
    }

    pub fn get_ref<'a>(&self, world: &'a World) -> Option<&'a dyn ScriptResource> {
        (self.ref_func)(world)
    }

    pub fn as_mut<'a>(&mut self, world: &'a mut World) -> &'a mut dyn ScriptResource {
        (self.mut_func)(world).expect("resource present")
    }

    pub fn get_mut<'a>(&mut self, world: &'a mut World) -> Option<&'a mut dyn ScriptResource> {
        (self.mut_func)(world)
    }
}

/*
pub fn make_resource_lookup_mut<T>() -> Arc<ResourceLookupMutFunc>
where
    T: Resource + ScriptResource + 'static,
{
    Arc::new(move |world: &mut World| {
        if let Some(ptr) = world.get_resource_mut::<T>() {
            let rto: &mut (dyn ScriptResource + 'static) = ptr.into_inner();
            Some(rto)
        } else {
            None
        }
    })
}
 */

/// Use #[derive(NitrousComponent)] to implement this trait. The derived implementation
/// will expect the struct to have an impl block annotated with #[inject_nitrous]. This
/// second macro will use #[method] tags to populate lookups for the various operations.
pub trait ScriptComponent: Send + Sync + 'static {
    fn component_name(&self) -> &'static str;
    fn call_method(&mut self, entity: Entity, name: &str, args: &[Value]) -> Result<Value>;
    fn put(&mut self, entity: Entity, name: &str, value: Value) -> Result<()>;
    fn get(&self, entity: Entity, name: &str) -> Result<Value>;
    fn names(&self) -> Vec<&str>;
}

pub type ComponentLookupMutFunc = dyn Fn(Entity, &mut World) -> Option<&mut (dyn ScriptComponent + 'static)>
    + Send
    + Sync
    + 'static;

pub fn make_component_lookup_mut<T>() -> Arc<ComponentLookupMutFunc>
where
    T: Component + ScriptComponent + 'static,
{
    Arc::new(move |entity: Entity, world: &mut World| {
        if let Some(ptr) = world.get_mut::<T>(entity) {
            let cto: &mut (dyn ScriptComponent + 'static) = ptr.into_inner();
            Some(cto)
        } else {
            None
        }
    })
}

/// An inline function that can be stuffed into a Value, where needed.
pub type RustCallbackFunc = dyn Fn(&[Value], &mut World) -> Value + Send + Sync + 'static;

#[derive(Default)]
struct EntityMetadata {
    components: HashMap<String, Arc<ComponentLookupMutFunc>>,
}

impl EntityMetadata {
    fn component_names(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(|s| s.as_str())
    }
}

/// A map from names to pointers into World.
#[derive(Default)]
pub struct WorldIndex {
    resource_ptrs: HashMap<String, ResourceLookup>,
    named_entities: HashMap<String, Entity>,
    entity_metadata: HashMap<Entity, EntityMetadata>,
}

impl WorldIndex {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn insert_named_resource<T>(&mut self, name: String)
    where
        T: Resource + ScriptResource + 'static,
    {
        assert!(!self.resource_ptrs.contains_key(&name));
        self.resource_ptrs.insert(name, ResourceLookup::new::<T>());
    }

    pub fn lookup_resource(&self, name: &str) -> Option<&ResourceLookup> {
        self.resource_ptrs.get(name)
    }

    pub fn resource_names(&self) -> impl Iterator<Item = &str> {
        self.resource_ptrs.keys().map(|s| s.as_str())
    }

    pub fn insert_named_entity<S>(&mut self, entity_name: S, entity: Entity) -> Result<()>
    where
        S: Into<String>,
    {
        let entity_name = entity_name.into();
        ensure!(
            !self.named_entities.contains_key(&entity_name),
            "duplicate entity name"
        );
        self.named_entities.insert(entity_name, entity);
        self.entity_metadata
            .insert(entity, EntityMetadata::default());
        Ok(())
    }

    pub fn insert_named_component(
        &mut self,
        entity: Entity,
        component_name: &str,
        lookup: Arc<ComponentLookupMutFunc>,
    ) -> Result<()> {
        ensure!(self.entity_metadata.contains_key(&entity));
        let meta = self
            .entity_metadata
            .get_mut(&entity)
            .ok_or_else(|| anyhow!("entity {:?} is not a script entity", entity))?;
        ensure!(
            !meta.components.contains_key(component_name),
            format!("duplicate component name: {}", component_name)
        );
        meta.components.insert(component_name.to_owned(), lookup);
        Ok(())
    }

    pub fn entity_names(&self) -> impl Iterator<Item = &str> {
        self.named_entities.keys().map(|s| s.as_str())
    }

    pub fn entity_component_names(&self, entity: Entity) -> Option<impl Iterator<Item = &str>> {
        self.entity_metadata
            .get(&entity)
            .map(|components| components.component_names())
    }

    pub fn entity_component_attrs(
        &self,
        entity: Entity,
        component_name: &str,
        world: &mut World,
    ) -> Option<Vec<String>> {
        self.entity_metadata
            .get(&entity)
            .map(|components| {
                components
                    .components
                    .get(component_name)
                    .map(|lookup| {
                        lookup(entity, world).map(|attrs| {
                            attrs
                                .names()
                                .iter()
                                .map(|&s| s.to_owned())
                                .collect::<Vec<String>>()
                        })
                    })
                    .flatten()
            })
            .flatten()
    }

    /// Look up a named entity in the index.
    pub fn lookup_entity(&self, name: &str) -> Option<Value> {
        self.named_entities
            .get(name)
            .map(|entity| Value::new_entity(*entity))
    }

    /// Look up a named component within an entity.
    pub fn lookup_component(&self, entity: &Entity, name: &str) -> Option<Value> {
        self.entity_metadata
            .get(entity)
            .map(|comps| {
                comps
                    .components
                    .get(name)
                    .map(|lookup| Value::new_component(*entity, lookup.to_owned()))
            })
            .flatten()
    }

    pub fn entity_components(&self, entity: &Entity) -> Option<impl Iterator<Item = &str>> {
        self.entity_metadata
            .get(entity)
            .map(|comps| comps.components.keys().map(|k| k.as_ref()))
    }

    pub fn component_attrs(&self, entity: &Entity) -> Vec<&str> {
        self.entity_metadata
            .get(entity)
            .map(|comps| {
                comps
                    .components
                    .keys()
                    .map(|v| v.as_str())
                    .collect::<Vec<&str>>()
            })
            .unwrap_or_else(Vec::new)
    }
}

/// A simple name <-> value map
#[derive(Clone, Debug)]
pub struct LocalNamespace {
    memory: HashMap<String, Value>,
}

impl From<HashMap<String, Value>> for LocalNamespace {
    fn from(memory: HashMap<String, Value>) -> Self {
        Self { memory }
    }
}

impl From<HashMap<&str, Value>> for LocalNamespace {
    fn from(mut memory: HashMap<&str, Value>) -> Self {
        memory
            .drain()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<HashMap<String, Value>>()
            .into()
    }
}

impl LocalNamespace {
    pub fn empty() -> Self {
        Self {
            memory: HashMap::new(),
        }
    }

    #[inline]
    pub fn put_if_absent(&mut self, name: &str, value: Value) -> &mut Self {
        if !self.memory.contains_key(name) {
            self.memory.insert(name.to_owned(), value);
        }
        self
    }

    #[inline]
    pub fn put<S: Into<String>>(&mut self, name: S, value: Value) -> &mut Self {
        self.memory.insert(name.into(), value);
        self
    }

    #[inline]
    pub fn get(&self, name: &str) -> Option<Value> {
        self.memory.get(name).cloned()
    }

    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.memory.contains_key(name)
    }

    #[inline]
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.memory.remove(name)
    }
}
