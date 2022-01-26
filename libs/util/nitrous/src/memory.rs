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
use anyhow::Result;
use parking_lot::RwLock;
use std::{collections::HashMap, fmt::Debug, mem::transmute, sync::Arc};

/// Implement this interface and store as a module in the Runtime with insert_module
/// in order for the exposed functionality to be exposed to scripts. The helper macros
/// in nitrous_injector make this as simple as adding a #[method] attribute, in most cases.
pub trait Module: Debug + Send + Sync + 'static {
    // Note: manually passing the module here until Rust has arbitrary self.
    //       This detail is largely papered over via macros and scripting.
    fn module_name(&self) -> String;
    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value>;
    fn put(&mut self, module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Result<()>;
    fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Result<Value>;
    fn names(&self) -> Vec<&str>;
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
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.memory.remove(name)
    }
}

/// A blank slate that we can cast into and out of a &dyn Module trait object.
///
/// Safety: No, definitely not.
///
///     Bevy doesn't expose raw pointers and we wouldn't want it if it did.
///     What we actually need is a trait object: the composite of the pointer
///     to the block of memory, plus the vtable for the pointed to trait's
///     code. Since all we have is the name in scripts, not the type, we have
///     a bit of a problem. Instead of the type we use get_resource to return
///     a reference to the opaque block of memory right after we insert it,
///     cast it to the trait object, then transmute the memory of that trait
///     object into this bad idea.
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
#[derive(Copy, Clone, Debug)]
pub(crate) struct ResourceTraitObject {
    bad_idea_ptr: usize,
    bad_idea_meta: usize,
}

impl ResourceTraitObject {
    pub fn from_module(module: &dyn Module) -> Self {
        unsafe { transmute(module) }
    }

    pub fn to_module(self) -> &'static dyn Module {
        unsafe { transmute(self) }
    }
}

/// A map from names to pointers into World.
pub struct ResourceNamespace {
    resource_ptrs: HashMap<String, ResourceTraitObject>,
}

impl ResourceNamespace {
    pub fn empty() -> Self {
        Self {
            resource_ptrs: HashMap::new(),
        }
    }

    pub fn insert_named_resource<S: Into<String>>(&mut self, name: S, resource: &dyn Module) {
        // Safety:
        // The resource of type T is stored as the first value in a unique_component Column,
        // represented as a BlobVec, where it is the first and only allocation. The allocation
        // was made with std::alloc::alloc, and will only be reallocated if the BlobVec Grows.
        // It will not grow, since this is a unique_component.
        //
        // As such, we can cast it to the &dyn Module above, then transmute to and from TraitObject
        // safely, as long as the underlying allocation never changes. Since modules are permanent
        // and tied to the world and runtime, we will stop running scripts (via the runtime's
        // scheduler) before deallocating the Runtime's World, and thus the storage.
        // let module_trait_obj = resource as &dyn Module;
        self.resource_ptrs
            .insert(name.into(), ResourceTraitObject::from_module(resource));
    }

    pub fn lookup(&self, name: &str) -> Option<&dyn Module> {
        self.resource_ptrs.get(name).map(|v| v.to_module())
    }
}
