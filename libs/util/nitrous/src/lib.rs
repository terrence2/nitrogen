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
pub mod ir;
mod lower;
mod memory;
mod script;
mod value;

pub use crate::{
    ast::NitrousAst,
    exec::{ExecutionContext, NitrousExecutor, YieldState},
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

/*
impl GlobalNamespace {
    pub fn new() -> Arc<RwLock<Self>> {
        let obj = Arc::new(RwLock::new(Self {
            memory: HashMap::new(),
        }));
        obj.write().memory.insert(
            "help".to_owned(),
            Value::Method(obj.clone(), "help".to_owned()),
        );
        obj
    }

    pub fn put_global(&mut self, name: &str, value: Value) {
        self.memory.insert(name.to_owned(), value);
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.memory.get(name).cloned()
    }

    pub fn format_help(&self) -> Value {
        let mut records = self
            .memory
            .iter()
            .map(|(k, v)| match v {
                Value::Module(m) => (
                    0,
                    k.to_owned(),
                    format!("[{}]", k),
                    m.0.to_module().module_name(),
                ),
                Value::Method(_, name) => (
                    1,
                    k.to_owned(),
                    if name == "help" {
                        "help()".to_owned()
                    } else {
                        v.to_string()
                    },
                    "show this message".to_owned(),
                ),
                _ => (1, k.to_owned(), k.to_owned(), v.to_string()),
            })
            .collect::<Vec<_>>();
        records.sort();

        let mut width = 0;
        for (_, _, k, _) in &records {
            width = width.max(k.len());
        }
        let mut out = String::new();
        for (_, _, k, v) in &records {
            out += &format!("{:0width$} - {}\n", k, v, width = width);
        }
        Value::String(out)
    }
}

impl Module for GlobalNamespace {
    fn module_name(&self) -> String {
        "Interpreter".to_owned()
    }

    fn call_method(&mut self, name: &str, _args: &[Value]) -> Result<Value> {
        ensure!(self.memory.contains_key(name));
        Ok(match name {
            "help" => self.format_help(),
            _ => bail!("unknown method named: {}", name),
        })
    }

    fn put(&mut self, _module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Result<()> {
        self.memory.insert(name.to_owned(), value);
        Ok(())
    }

    fn get(&self, _module: Arc<RwLock<dyn Module>>, name: &str) -> Result<Value> {
        match self.memory.get(name) {
            Some(v) => Ok(v.to_owned()),
            None => bail!(
                "lookup of unknown property '{}' in '{}'",
                name,
                self.module_name()
            ),
        }
    }

    fn names(&self) -> Vec<&str> {
        self.memory.keys().map(|v| v.as_str()).collect()
    }
}
 */
