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
    memory::{ComponentLookup, ScriptComponent, ScriptResource, WorldIndex},
    value::Value,
};
use anyhow::Result;
use bevy_ecs::{
    prelude::*,
    query::WorldQuery,
    system::Resource,
    world::{EntityMut, EntityRef},
};

#[derive(Component, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EntityName(String);

impl EntityName {
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Wraps an EntityMut to provide named creation methods
pub struct NamedEntityMut<'w> {
    entity: EntityMut<'w>,
}

impl<'w> NamedEntityMut<'w> {
    pub fn id(&self) -> Entity {
        self.entity.id()
    }

    pub fn insert<T>(&mut self, value: T) -> &mut Self
    where
        T: Component,
    {
        self.entity.insert(value);
        self
    }

    pub fn insert_named<T>(&mut self, value: T) -> Result<&mut Self>
    where
        T: Component + ScriptComponent + 'static,
    {
        let component_name = value.component_name();

        // Record the component in the store.
        self.entity.insert(value);

        // Index the component in the script engine.
        // Safety: this is safe because NamedEntityEntity contains a field borrowed from World.
        //         As such, you cannot get to a NamedEntityEntity from just the world, so we
        //         cannot be entering here through some world-related path, such as a system.
        let entity = self.entity.id(); // Copy to avoid double-borrow
        unsafe { self.entity.world_mut() }
            .get_resource_mut::<WorldIndex>()
            .unwrap()
            .insert_named_component(entity, component_name, ComponentLookup::new::<T>())?;
        Ok(self)
    }

    pub fn remove<T>(&mut self) -> &mut Self
    where
        T: Component + 'static,
    {
        self.entity.remove::<T>();
        self
    }

    pub fn remove_named<T>(&mut self, component_name: &str) -> Result<()>
    where
        T: Component + ScriptComponent + 'static,
    {
        let id = self.id();
        unsafe { self.entity.world_mut() }
            .get_resource_mut::<WorldIndex>()
            .unwrap()
            .remove_named_component(id, component_name);
        self.entity.remove::<T>();
        Ok(())
    }

    pub fn rename(&mut self, target_name: &str) {
        let id = self.id();
        let mut index = unsafe { self.entity.world_mut() }.resource_mut::<WorldIndex>();
        let own_name = index.lookup_entity_name(id).unwrap();
        index.rename_entity(&own_name, target_name);
    }

    pub fn rename_numbered(&mut self, base_name: &str) {
        let id = self.id();
        let mut index = unsafe { self.entity.world_mut() }.resource_mut::<WorldIndex>();
        let mut i = 0;
        loop {
            let target_name = format!("{}{}", base_name, i);
            i += 1;
            if index.get_entity(&target_name).is_some() {
                continue;
            }
            let own_name = index.lookup_entity_name(id).unwrap();
            index.rename_entity(&own_name, target_name);
            break;
        }
    }
}

macro_rules! impl_immutable_heap_methods {
    () => {
        #[inline]
        pub fn world(&self) -> &World {
            &self.world
        }

        // Component Access
        #[inline]
        pub fn get<T: Component + 'static>(&self, entity: Entity) -> &T {
            self.world.get::<T>(entity).expect("entity not found")
        }

        #[inline]
        pub fn maybe_get<T: Component + 'static>(&self, entity: Entity) -> Option<&T> {
            self.world.get::<T>(entity)
        }

        #[inline]
        pub fn get_named<T: Component + 'static>(&self, name: &str) -> &T {
            let entity = self
                .resource::<WorldIndex>()
                .get_entity(name)
                .expect("named entity not found");
            self.get::<T>(entity)
        }

        #[inline]
        pub fn maybe_get_named<T: Component + 'static>(&self, name: &str) -> Option<&T> {
            if let Some(entity) = self.resource::<WorldIndex>().get_entity(name) {
                self.maybe_get::<T>(entity)
            } else {
                None
            }
        }

        #[inline]
        pub fn maybe_component_value_by_name(&self, entity: Entity, name: &str) -> Option<Value> {
            self.resource::<WorldIndex>()
                .lookup_component(&entity, name)
        }

        // Entity Access
        //   name to id
        #[inline]
        pub fn entity_by_name(&self, name: &str) -> Entity {
            if let Some(Value::Entity(entity)) = self.resource::<WorldIndex>().lookup_entity(name) {
                entity
            } else {
                panic!("no entity named {}", name)
            }
        }

        #[inline]
        pub fn maybe_entity_by_name(&self, name: &str) -> Option<Entity> {
            if let Some(Value::Entity(entity)) = self.resource::<WorldIndex>().lookup_entity(name) {
                Some(entity)
            } else {
                None
            }
        }

        //    entity to wrappers
        #[inline]
        pub fn entity(&self, entity: Entity) -> EntityRef {
            self.world.entity(entity)
        }

        #[inline]
        pub fn maybe_resource<T: Resource>(&self) -> Option<&T> {
            self.world.get_resource()
        }

        #[inline]
        pub fn resource<T: Resource>(&self) -> &T {
            self.world.get_resource().expect("unset resource")
        }

        #[inline]
        pub fn maybe_resource_by_name(&self, name: &str) -> Option<&dyn ScriptResource> {
            self.resource::<WorldIndex>()
                .lookup_resource(name)
                .map(|lookup| lookup.get_ref(self.world()))
                .flatten()
        }

        #[inline]
        pub fn maybe_resource_value_by_name(&self, name: &str) -> Option<Value> {
            self.resource::<WorldIndex>()
                .lookup_resource(name)
                .map(|lookup| Value::new_resource(lookup))
        }

        // FIXME: this should be immutable
        #[inline]
        pub fn resource_by_name(&self, name: &str) -> &dyn ScriptResource {
            self.maybe_resource_by_name(name)
                .expect("missing named resource")
        }

        #[inline]
        pub fn resource_names(&self) -> impl Iterator<Item = &str> {
            self.resource::<WorldIndex>().resource_names()
        }

        #[inline]
        pub fn entity_names(&self) -> impl Iterator<Item = &str> {
            self.resource::<WorldIndex>().entity_names()
        }

        #[inline]
        pub fn entity_component_names(&self, entity: Entity) -> Option<impl Iterator<Item = &str>> {
            self.resource::<WorldIndex>().entity_component_names(entity)
        }

        #[inline]
        pub fn entity_component_attrs(
            &self,
            entity: Entity,
            component: &str,
        ) -> Option<Vec<String>> {
            self.resource::<WorldIndex>()
                .entity_component_attrs(entity, component, &self.world)
        }
    };
}

macro_rules! impl_mutable_heap_methods {
    () => {
        #[inline]
        pub fn world_mut(&mut self) -> &mut World {
            &mut self.world
        }

        // Manage entities
        #[inline]
        pub fn spawn(&mut self) -> EntityMut {
            self.world.spawn()
        }

        #[inline]
        pub fn spawn_named<S>(&mut self, name: S) -> Result<NamedEntityMut>
        where
            S: Into<String>,
        {
            let name = name.into();

            // World is borrowed mutably here, so we can't reborrow anything in either
            // world or self, annoyingly.
            let mut ent_mut = self.world.spawn();
            ent_mut.insert(EntityName(name.clone()));
            let entity = ent_mut.id();

            // But we can go through ent_mut to get the already-borrowed world, as long as
            // we know it's safe to do so. Which it is as the unsafe share is the one above.
            unsafe { ent_mut.world_mut() }
                .get_resource_mut::<WorldIndex>()
                .unwrap()
                .insert_named_entity(name, entity)?;
            Ok(NamedEntityMut { entity: ent_mut })
        }

        #[inline]
        pub fn despawn(&mut self, entity: Entity) {
            self.resource_mut::<WorldIndex>().remove_entity(&entity);
            self.world.despawn(entity);
        }

        #[inline]
        pub fn despawn_named<S>(&mut self, name: S)
        where
            S: AsRef<str>,
        {
            if let Some(entity) = self.maybe_entity_by_name(name.as_ref()) {
                self.despawn(entity);
            }
        }

        #[inline]
        pub fn entity_mut(&mut self, entity: Entity) -> EntityMut {
            self.world.entity_mut(entity)
        }

        #[inline]
        pub fn named_entity_mut(&mut self, entity: Entity) -> NamedEntityMut {
            NamedEntityMut {
                entity: self.world.entity_mut(entity),
            }
        }

        // Manage components
        #[inline]
        pub fn get_mut<T: Component + 'static>(&mut self, entity: Entity) -> Mut<T> {
            self.world.get_mut::<T>(entity).expect("entity not found")
        }

        #[inline]
        pub fn maybe_get_mut<T: Component + 'static>(&mut self, entity: Entity) -> Option<Mut<T>> {
            self.world.get_mut::<T>(entity)
        }

        #[inline]
        pub fn get_named_mut<T: Component + 'static>(&mut self, name: &str) -> Mut<T> {
            let entity = self
                .resource::<WorldIndex>()
                .get_entity(name)
                .expect("named entity not found");
            self.get_mut::<T>(entity)
        }

        #[inline]
        pub fn maybe_get_named_mut<T: Component + 'static>(
            &mut self,
            name: &str,
        ) -> Option<Mut<T>> {
            let entity = self
                .resource::<WorldIndex>()
                .get_entity(name)
                .expect("named entity not found");
            self.maybe_get_mut::<T>(entity)
        }

        // Resource Management
        #[inline]
        pub fn insert_named_resource<S, T>(&mut self, name: S, value: T) -> &mut Self
        where
            S: Into<String>,
            T: ScriptResource + 'static,
        {
            self.world.insert_resource(value);
            self.resource_mut::<WorldIndex>()
                .insert_named_resource::<T>(name.into());
            self
        }

        #[inline]
        pub fn insert_resource<T: Resource>(&mut self, value: T) -> &mut Self {
            self.world.insert_resource(value);
            self
        }

        #[inline]
        pub fn insert_non_send_resource<T: 'static>(&mut self, value: T) -> &mut Self {
            self.world.insert_non_send_resource(value);
            self
        }

        #[inline]
        pub fn resource_mut<T: Resource>(&mut self) -> Mut<T> {
            self.world.get_resource_mut().expect("unset resource")
        }

        #[inline]
        pub fn maybe_resource_mut<T: Resource>(&mut self) -> Option<Mut<T>> {
            self.world.get_resource_mut()
        }

        #[inline]
        pub fn remove_resource<T: Resource>(&mut self) -> Option<T> {
            self.world.remove_resource()
        }

        #[inline]
        pub fn resource_scope<T: Resource, U>(
            &mut self,
            f: impl FnOnce(HeapMut, Mut<T>) -> U,
        ) -> U {
            self.world
                .resource_scope(|world, t: Mut<T>| f(HeapMut::wrap(world), t))
        }

        #[inline]
        pub fn query<Q>(&mut self) -> QueryState<Q, ()>
        where
            Q: WorldQuery,
        {
            self.world.query::<Q>()
        }
    };
}

/// An immutable wrapper around World that provides better and name-based accessors.
#[derive(Copy, Clone)]
pub struct HeapRef<'a> {
    world: &'a World,
}

impl<'a> HeapRef<'a> {
    #[inline]
    pub fn wrap(world: &'a World) -> Self {
        Self { world }
    }

    impl_immutable_heap_methods!();
}

/// A mutable wrapper around World that provides named-based scriptable creation methods.
pub struct HeapMut<'a> {
    world: &'a mut World,
}

impl<'a> HeapMut<'a> {
    #[inline]
    pub fn wrap(world: &'a mut World) -> Self {
        Self { world }
    }

    #[inline]
    pub fn as_ref(&self) -> HeapRef {
        HeapRef::wrap(self.world)
    }

    #[inline]
    pub fn as_mut(&mut self) -> HeapMut {
        HeapMut::wrap(self.world)
    }

    impl_immutable_heap_methods!();
    impl_mutable_heap_methods!();
}

/// A wrapper around world that provides name-based scriptable creation methods.
pub struct Heap {
    world: World,
}

impl Default for Heap {
    fn default() -> Self {
        let mut world = World::default();
        world.insert_resource(WorldIndex::default());
        Self { world }
    }
}

impl Heap {
    impl_immutable_heap_methods!();
    impl_mutable_heap_methods!();
}
