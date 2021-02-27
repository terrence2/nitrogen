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
use failure::{bail, Fallible};
use parking_lot::RwLock;
use std::{collections::HashMap, fmt::Debug, sync::Arc};

// Note: manually passing the module until we have arbitrary self.
pub trait Module: Debug {
    fn module_name(&self) -> String;
    fn call_method(&mut self, name: &str, args: &[Value]) -> Fallible<Value>;
    fn put(&mut self, module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Fallible<()>;
    fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Fallible<Value>;
}

#[derive(Debug)]
pub struct Interpreter {
    memory: HashMap<String, Value>,
    locals: HashMap<String, Value>,
}

impl Interpreter {
    pub fn boot() -> Self {
        Self {
            memory: HashMap::new(),
            locals: HashMap::new(),
        }
    }

    pub fn init(self) -> Fallible<Arc<RwLock<Self>>> {
        Ok(Arc::new(RwLock::new(self)))
    }

    pub fn with_locals<F>(&mut self, locals: &[(&str, Value)], callback: F) -> Fallible<Value>
    where
        F: Fn(&mut Interpreter) -> Fallible<Value>,
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

    pub fn interpret_once(&mut self, raw_script: &str) -> Fallible<Value> {
        self.interpret(&Script::compile(raw_script)?)
    }

    pub fn interpret(&mut self, script: &Script) -> Fallible<Value> {
        use std::borrow::Borrow;
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

    fn interpret_expr(&self, expr: &Expr) -> Fallible<Value> {
        Ok(match expr {
            Expr::Term(term) => match term {
                Term::Boolean(b) => Value::Boolean(*b),
                Term::Float(f) => Value::Float(*f),
                Term::Integer(i) => Value::Integer(*i),
                Term::String(s) => Value::String(s.to_owned()),
                Term::Symbol(sym) => {
                    if let Some(v) = self.locals.get(sym) {
                        v.clone()
                    } else if let Some(v) = self.memory.get(sym) {
                        v.clone()
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

// The interpreter is also the root namespace.
impl Module for Interpreter {
    fn module_name(&self) -> String {
        "Interpreter".to_owned()
    }

    fn call_method(&mut self, _name: &str, _args: &[Value]) -> Fallible<Value> {
        bail!("no methods are defined on the interpreter")
    }

    fn put(&mut self, _module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Fallible<()> {
        self.memory.insert(name.to_owned(), value);
        Ok(())
    }

    fn get(&self, _module: Arc<RwLock<dyn Module>>, name: &str) -> Fallible<Value> {
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
    fn test_interpret_basic() -> Fallible<()> {
        let mut interpreter = Interpreter::boot();
        let script = Script::compile("2 + 2")?;
        assert_eq!(interpreter.interpret(&script)?, Value::Integer(4));
        Ok(())
    }
}
