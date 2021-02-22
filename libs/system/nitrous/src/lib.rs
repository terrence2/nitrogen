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

pub use crate::script::Script;

use crate::ir::{Expr, Operator, Term};
use failure::{bail, Fallible};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::{collections::HashMap, fmt, fmt::Debug, ops, sync::Arc};

// Note: manually passing the module until we have arbitrary self.
pub trait Module: Debug {
    fn module_name(&self) -> String;
    fn call_method(&mut self, name: &str, args: &[Value]) -> Fallible<Value>;
    fn put(&mut self, module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Fallible<()>;
    fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Fallible<Value>;
}

#[derive(Clone, Debug)]
pub enum Value {
    Boolean(bool),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
    Module(Arc<RwLock<dyn Module>>),
    Method(Arc<RwLock<dyn Module>>, String), // TODO: atoms
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
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(v) => write!(f, "{}", v),
            Self::Integer(v) => write!(f, "{}", v),
            Self::Float(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "\"{}\"", v),
            Self::Module(v) => write!(f, "{}", v.read().module_name()),
            Self::Method(v, name) => write!(f, "{}.{}", v.read().module_name(), name),
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
            Self::Module(_) => false,
            Self::Method(_, _) => false,
        }
    }
}

impl Eq for Value {}

impl Value {
    pub fn impl_multiply(self, other: Self) -> Fallible<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs * rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) * rhs),
                Value::String(_) => bail!("cannot multiply a number by a string"),
                Value::Boolean(_) => bail!("cannot multiply a number to a boolean"),
                Value::Module(_) => bail!("cannot multiply a number by a module"),
                Value::Method(_, _) => bail!("cannot multiply a number by a method"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs * OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs * rhs),
                Value::String(_) => bail!("cannot multiply a number by a string"),
                Value::Boolean(_) => bail!("cannot multiply a number to a boolean"),
                Value::Module(_) => bail!("cannot multiply a number by a module"),
                Value::Method(_, _) => bail!("cannot multiply a number by a method"),
            },
            Value::String(lhs) => match other {
                Value::Integer(rhs) => Value::String(lhs.repeat(rhs.max(0) as usize)),
                Value::Float(rhs) => Value::String(lhs.repeat(rhs.floor().max(0f64) as usize)),
                Value::String(_) => bail!("cannot multiply a string by a string"),
                Value::Boolean(_) => bail!("cannot multiply a number to a boolean"),
                Value::Module(_) => bail!("cannot multiply a string by a module"),
                Value::Method(_, _) => bail!("cannot multiply a string by a method"),
            },
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
        })
    }

    pub fn impl_divide(self, other: Self) -> Fallible<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs / rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) / rhs),
                Value::String(_) => bail!("cannot divide a number by a string"),
                Value::Boolean(_) => bail!("cannot divide a number by a boolean"),
                Value::Module(_) => bail!("cannot divide a number by a module"),
                Value::Method(_, _) => bail!("cannot divide a number by a method"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs / OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs / rhs),
                Value::String(_) => bail!("cannot divide a number by a string"),
                Value::Boolean(_) => bail!("cannot divide a number by a boolean"),
                Value::Module(_) => bail!("cannot divide a number by a module"),
                Value::Method(_, _) => bail!("cannot divide a number by a method"),
            },
            Value::String(_) => bail!("cannot divide a string by anything"),
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
        })
    }

    pub fn impl_add(self, other: Self) -> Fallible<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs + rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) + rhs),
                Value::String(_) => bail!("cannot add a string to a number"),
                Value::Boolean(_) => bail!("cannot add a number to a boolean"),
                Value::Module(_) => bail!("cannot add a module to a number"),
                Value::Method(_, _) => bail!("cannot add a method to a number"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs + OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs + rhs),
                Value::String(_) => bail!("cannot add a string to a number"),
                Value::Boolean(_) => bail!("cannot add a number to a boolean"),
                Value::Module(_) => bail!("cannot add a module to a number"),
                Value::Method(_, _) => bail!("cannot add a method to a number"),
            },
            Value::String(lhs) => match other {
                Value::Integer(_) => bail!("cannot add a number to a string"),
                Value::Float(_) => bail!("cannot add a number to a string"),
                Value::String(rhs) => Value::String(lhs + &rhs),
                Value::Boolean(_) => bail!("cannot add a number to a boolean"),
                Value::Module(_) => bail!("cannot add a module to a string"),
                Value::Method(_, _) => bail!("cannot add a method to a string"),
            },
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
        })
    }

    pub fn impl_subtract(self, other: Self) -> Fallible<Self> {
        Ok(match self {
            Value::Integer(lhs) => match other {
                Value::Integer(rhs) => Value::Integer(lhs - rhs),
                Value::Float(rhs) => Value::Float(OrderedFloat(lhs as f64) - rhs),
                Value::String(_) => bail!("cannot subtract a string from a number"),
                Value::Boolean(_) => bail!("cannot subtract a boolean from a number"),
                Value::Module(_) => bail!("cannot subtract a module from a number"),
                Value::Method(_, _) => bail!("cannot subtract a method from a number"),
            },
            Value::Float(lhs) => match other {
                Value::Integer(rhs) => Value::Float(lhs - OrderedFloat(rhs as f64)),
                Value::Float(rhs) => Value::Float(lhs - rhs),
                Value::String(_) => bail!("cannot subtract a string from a number"),
                Value::Boolean(_) => bail!("cannot subtract a boolean from a number"),
                Value::Module(_) => bail!("cannot subtract a module from a number"),
                Value::Method(_, _) => bail!("cannot subtract a method from a number"),
            },
            Value::String(_) => bail!("cannot subtract with a string"),
            Value::Boolean(_) => bail!("cannot do arithmetic on a boolean"),
            Value::Module(_) => bail!("cannot do arithmetic on a module"),
            Value::Method(_, _) => bail!("cannot do arithmetic on a method"),
        })
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InterpreterStatus {
    Continue,
    Exit,
}

impl InterpreterStatus {
    pub fn should_exit(&self) -> bool {
        matches!(self, Self::Exit)
    }
}

impl Default for InterpreterStatus {
    fn default() -> Self {
        Self::Continue
    }
}

impl ops::BitOrAssign for InterpreterStatus {
    fn bitor_assign(&mut self, rhs: Self) {
        if rhs == Self::Exit {
            *self = Self::Exit;
        }
    }
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

    pub fn wrapped(self) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(self))
    }

    pub fn with_local<F>(&mut self, name: &str, value: Value, callback: F) -> Fallible<Value>
    where
        F: Fn(&Interpreter) -> Fallible<Value>,
    {
        self.locals.insert(name.to_owned(), value);
        let result = callback(self);
        self.locals.remove(name);
        result
    }

    pub fn interpret(&self, script: &Script) -> Fallible<Value> {
        self.interpret_expr(&script.expr)
    }

    fn interpret_expr(&self, expr: &Expr) -> Fallible<Value> {
        Ok(match expr {
            Expr::Term(term) => match term {
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
        let interpreter = Interpreter::boot();
        let script = Script::compile_expr("2 + 2")?;
        assert_eq!(interpreter.interpret(&script)?, Value::Integer(4));
        Ok(())
    }
}
