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
mod ir;
mod script;
mod value;

pub use crate::{script::Script, value::Value};

use crate::ir::{Expr, Operator, Stmt, Term};
use anyhow::{bail, ensure, Result};
use log::debug;
use parking_lot::RwLock;
use std::{borrow::Borrow, collections::HashMap, fmt::Debug, sync::Arc};

// Note: manually passing the module until we have arbitrary self.
pub trait Module: Debug + Send + Sync + 'static {
    fn module_name(&self) -> String;
    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value>;
    fn put(&mut self, module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Result<()>;
    fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Result<Value>;
}

#[derive(Debug)]
pub struct Interpreter {
    locals: HashMap<String, Value>,
    globals: Arc<RwLock<GlobalNamespace>>,
}

impl Interpreter {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            locals: HashMap::new(),
            globals: GlobalNamespace::new(),
        }))
    }

    pub fn with_locals<F>(&mut self, locals: &[(&str, Value)], mut callback: F) -> Result<Value>
    where
        F: FnMut(&mut Interpreter) -> Result<Value>,
    {
        for (name, value) in locals {
            self.locals.insert((*name).to_owned(), value.to_owned());
        }
        let result = callback(self);
        for (name, _) in locals {
            self.locals.remove(*name);
        }
        result
    }

    pub fn put_global(&mut self, name: &str, value: Value) {
        self.globals.write().put_global(name, value);
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.globals.write().get_global(name)
    }

    pub fn interpret_once(&mut self, raw_script: &str) -> Result<Value> {
        self.interpret(&Script::compile(raw_script)?)
    }

    pub fn interpret(&mut self, script: &Script) -> Result<Value> {
        debug!("Interpret: {}", script);
        let mut out = Value::True();
        for stmt in script.statements() {
            match stmt.borrow() {
                Stmt::LetAssign(target, expr) => {
                    let result = self.interpret_expr(expr)?;
                    if let Term::Symbol(name) = target {
                        self.locals.insert(name.to_owned(), result);
                    }
                }
                Stmt::Expr(expr) => {
                    out = self.interpret_expr(expr)?;
                }
            }
        }
        Ok(out)
    }

    fn interpret_expr(&self, expr: &Expr) -> Result<Value> {
        Ok(match expr {
            Expr::Term(term) => match term {
                Term::Boolean(b) => Value::Boolean(*b),
                Term::Float(f) => Value::Float(*f),
                Term::Integer(i) => Value::Integer(*i),
                Term::String(s) => Value::String(s.to_owned()),
                Term::Symbol(sym) => {
                    if let Some(v) = self.locals.get(sym) {
                        v.clone()
                    } else if let Ok(v) = self.globals.read().get(self.globals.clone(), sym) {
                        v
                    } else {
                        bail!("Unknown symbol '{}'", sym)
                    }
                }
            },
            Expr::BinOp(lhs, op, rhs) => {
                let t0 = self.interpret_expr(lhs)?;
                let t1 = self.interpret_expr(rhs)?;
                match op {
                    Operator::Multiply => t0.impl_multiply(t1)?,
                    Operator::Divide => t0.impl_divide(t1)?,
                    Operator::Add => t0.impl_add(t1)?,
                    Operator::Subtract => t0.impl_subtract(t1)?,
                }
            }
            Expr::Attr(base, member) => match self.interpret_expr(base)? {
                Value::Module(ns) => match member {
                    Term::Symbol(sym) => ns.read().get(ns.clone(), sym)?,
                    _ => bail!("attribute expr member is not a symbol"),
                },
                _ => bail!("attribute expr base did not evaluate to a module"),
            },
            Expr::Call(base, args) => {
                let base = self.interpret_expr(base)?;
                let mut argvec = Vec::new();
                for arg in args {
                    argvec.push(self.interpret_expr(arg)?);
                }
                match base {
                    Value::Method(module, method_name) => {
                        module.write().call_method(&method_name, &argvec)?
                    }
                    _ => bail!("call must be on a method value"),
                }
            }
        })
    }
}

#[derive(Debug)]
pub struct GlobalNamespace {
    memory: HashMap<String, Value>,
}

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
                Value::Module(m) => (0, k.to_owned(), format!("[{}]", k), m.read().module_name()),
                Value::Method(_, name) => (
                    1,
                    k.to_owned(),
                    if name == "help" {
                        name.to_owned()
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
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_interpret_basic() -> Result<()> {
        let interpreter = Interpreter::new();
        let script = Script::compile("2 + 2")?;
        assert_eq!(interpreter.write().interpret(&script)?, Value::Integer(4));
        Ok(())
    }
}
