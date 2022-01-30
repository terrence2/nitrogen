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
use crate::value::Value;
use anyhow::{anyhow, ensure, Result};
use bevy_ecs::prelude::*;
use std::{collections::HashMap, fmt::Debug, mem::transmute, sync::Arc};

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

/// A blank slate that we can cast into and out of a &dyn ScriptResource trait object.
///
/// Safety: No, definitely not.
///
/// Bevy doesn't expose raw pointers and we wouldn't want it if it did.
/// What we actually need is a trait object: the composite of the pointer
/// to the block of memory, plus the vtable for the pointed to trait's
/// code. Since all we have is the name in scripts, not the type, we have
/// a bit of a problem. Instead of the type we use get_resource to return
/// a reference to the opaque block of memory right after we insert it,
/// cast it to the trait object, then transmute the memory of that trait
/// object into this bad idea.
///
/// Safety Bevy: We depend on bevy_ecs not moving the resource allocation. It
///              is stored in a manually allocated chunk as a BlobVec on a
///              column in a unique_component. It's not likely that this will
///              move, but yikes.
///
/// Safety Rust: We depend on the current shape of a trait object: note the usize
///              below that makes sure we generally get two pointers worth of data
///              with pointer alignment and endianness. If the size is wrong, the
///              transmute will at least fail, but there are lots of ways changes
///              to Rust's implementation could make this break.
///
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceTraitObject {
    bad_idea_ptr: usize,
    bad_idea_meta: usize,
}

impl ResourceTraitObject {
    pub fn from_resource(resource: &dyn ScriptResource) -> Self {
        unsafe { transmute(resource) }
    }

    pub fn to_resource(self) -> &'static mut dyn ScriptResource {
        unsafe { transmute(self) }
    }
}

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

/// Safety: hahahahahaha.... no
///
/// Used to escape world lifetime considerations.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct ComponentTraitObject {
    bad_idea_ptr: usize,
    bad_idea_meta: usize,
}

pub type ComponentLookupFunc =
    dyn Fn(Entity, &mut World) -> &mut (dyn ScriptComponent + 'static) + Send + Sync + 'static;

#[derive(Default)]
struct EntityMetadata {
    components: HashMap<String, Arc<ComponentLookupFunc>>,
}

/// A map from names to pointers into World.
#[derive(Default)]
pub struct WorldIndex {
    resource_ptrs: HashMap<String, ResourceTraitObject>,
    named_entities: HashMap<String, Entity>,
    entity_metadata: HashMap<Entity, EntityMetadata>,
}

impl WorldIndex {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn insert_named_resource<S: Into<String>>(
        &mut self,
        name: S,
        resource: &dyn ScriptResource,
    ) -> Result<()> {
        // Safety:
        // The resource of type T is stored as the first value in a unique_component Column,
        // represented as a BlobVec, where it is the first and only allocation. The allocation
        // was made with std::alloc::alloc, and will only be reallocated if the BlobVec Grows.
        // It will not grow, since this is a unique_component.
        //
        // As such, we can cast it to the &dyn ScriptResource above, then transmute to and from
        // TraitObject safely, as long as the underlying allocation never changes. Since modules are
        // permanent and tied to the world and runtime, we will stop running scripts (via the
        // runtime's scheduler) before deallocating the Runtime's World, and thus the storage.
        let name = name.into();
        ensure!(!self.resource_ptrs.contains_key(&name));
        self.resource_ptrs
            .insert(name, ResourceTraitObject::from_resource(resource));
        Ok(())
    }

    pub fn lookup_resource(&self, name: &str) -> Option<Value> {
        self.resource_ptrs
            .get(name)
            .map(|rto| Value::new_resource(*rto))
    }

    /// Register an entity with the index.
    // pub fn insert_named_entity<S: Into<String>>(&mut self, name: S, entity: Entity) -> Result<()> {
    //     let name = name.into();
    //     println!("INSERTING: {}", name);
    //     ensure!(!self.named_entities.contains_key(&name));
    //     self.named_entities.insert(name.into(), entity);
    //     self.entity_metadata
    //         .insert(entity, EntityMetadata::default());
    //     Ok(())
    // }

    pub fn upsert_named_component(
        &mut self,
        entity_name: &str,
        entity: Entity,
        component_name: &str,
        lookup: Arc<ComponentLookupFunc>,
    ) -> Result<()> {
        if !self.named_entities.contains_key(entity_name) {
            self.named_entities.insert(entity_name.to_owned(), entity);
            self.entity_metadata
                .insert(entity, EntityMetadata::default());
        }
        let meta = self
            .entity_metadata
            .get_mut(&entity)
            .ok_or_else(|| anyhow!("entity {:?} is not a script entity", entity))?;
        ensure!(!meta.components.contains_key(component_name));
        meta.components.insert(component_name.to_owned(), lookup);
        Ok(())
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
    pub fn put<S: Into<String>>(&mut self, name: S, value: Value) {
        self.memory.insert(name.into(), value);
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
