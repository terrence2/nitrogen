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
mod ast;
mod exec;
mod heap;
pub mod ir;
mod lower;
mod memory;
mod script;
mod value;

pub use crate::{
    ast::NitrousAst,
    exec::{ExecutionContext, NitrousExecutor, YieldState},
    heap::{Heap, HeapMut, HeapRef, NamedEntityMut},
    lower::{Instr, NitrousCode},
    memory::{
        make_component_lookup_mut, ComponentLookupMutFunc, LocalNamespace, ScriptComponent,
        ScriptResource, WorldIndex,
    },
    script::NitrousScript,
    value::Value,
};
pub use nitrous_injector::{
    getter, inject_nitrous_component, inject_nitrous_resource, method, setter, NitrousComponent,
    NitrousResource,
};
