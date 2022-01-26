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
    ast::NitrousAst,
    ir::{Expr, Operator, Stmt, Term},
    value::Value,
};
use anyhow::{bail, Result};
use std::collections::HashMap;

/// Nitrous uses a fairly standard stack-oriented VM.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Atom(u32);

#[derive(Clone, Debug)]
pub enum Instr {
    Push(Value),
    LoadLocal(Atom),
    StoreLocal(Atom),

    Multiply,
    Divide,
    Add,
    Subtract,

    Call(u32),
    Attr(Atom),
    Await,
}

/// Instructions, atoms, and any other resources need to represent the program in a stack machine.
#[derive(Clone, Debug)]
pub struct NitrousCode {
    code: Vec<Instr>,
    atoms_matcher: HashMap<String, Atom>,
    next_atom: u32,
}

impl NitrousCode {
    pub fn lower(ast: NitrousAst) -> Result<Self> {
        let mut code = Self {
            code: Vec::with_capacity(ast.statements().len() * 2),
            atoms_matcher: HashMap::new(),
            next_atom: 1,
        };
        for stmt in ast.statements() {
            code.lower_stmt(stmt)?;
        }
        Ok(code)
    }

    pub fn finish(mut self) -> Result<(Vec<Instr>, HashMap<Atom, String>)> {
        Ok((
            self.code,
            self.atoms_matcher.drain().map(|(k, v)| (v, k)).collect(),
        ))
    }

    fn upsert_atom(&mut self, symbol: &str) -> Atom {
        let atom = *self
            .atoms_matcher
            .entry(symbol.to_owned())
            .or_insert(Atom(self.next_atom));
        self.next_atom = atom.0.checked_add(1).expect("no overflow");
        atom
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::LetAssign(target, expr) => {
                self.lower_expr(expr)?;
                if let Term::Symbol(name) = target {
                    let atom = self.upsert_atom(name);
                    self.code.push(Instr::StoreLocal(atom));
                } else {
                    bail!("don't know how to assign to a target of {}", target);
                }
            }
            Stmt::Expr(expr) => {
                self.lower_expr(expr)?;
            }
        }
        Ok(())
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Term(term) => match term {
                Term::Boolean(b) => self.code.push(Instr::Push(Value::Boolean(*b))),
                Term::Float(f) => self.code.push(Instr::Push(Value::Float(*f))),
                Term::Integer(i) => self.code.push(Instr::Push(Value::Integer(*i))),
                Term::String(s) => self.code.push(Instr::Push(Value::String(s.to_owned()))),
                Term::Symbol(sym) => {
                    let atom = self.upsert_atom(sym);
                    self.code.push(Instr::LoadLocal(atom));
                    // if let Some(v) = self.locals.get(sym) {
                    //     v
                    // } else if let Some(&module_to) = self.modules.get(sym) {
                    //     // let any_module: &mut dyn Any =
                    //     //     world.get_resource_by_type_id_mut(typeid).unwrap();
                    //     // let module = any_module.downcast_ref::<dyn Module>().expect("non-module in the modules list");
                    //     // let failure = any_module.downcast_ref::<i32>().expect("this will fail");
                    //     // let module_ptr: *const dyn Module = unsafe { transmute(*trait_obj) };
                    //     //let module: &dyn Module = unsafe { transmute(module_ptr) };
                    //     let module = module_to.to_module();
                    //
                    //     bail!("found module: {}", module.module_name());
                    // } else {
                    //     bail!("Unknown symbol '{}'", sym)
                    // }
                }
            },
            Expr::BinOp(lhs, op, rhs) => {
                self.lower_expr(lhs)?;
                self.lower_expr(rhs)?;
                match op {
                    Operator::Multiply => self.code.push(Instr::Multiply),
                    Operator::Divide => self.code.push(Instr::Divide),
                    Operator::Add => self.code.push(Instr::Add),
                    Operator::Subtract => self.code.push(Instr::Subtract),
                }
            }
            Expr::Attr(base, member) => {
                self.lower_expr(base)?;
                if let Term::Symbol(sym) = member {
                    let atom = self.upsert_atom(sym);
                    self.code.push(Instr::Attr(atom));
                } else {
                    bail!(
                        "attribute member reference must be a symbol, not: {}",
                        member
                    );
                }
            }
            Expr::Await(expr) => {
                self.lower_expr(expr)?;
                self.code.push(Instr::Await);
                //block_on(result.to_future()?.write().as_mut())
            }
            Expr::Call(base, args) => {
                for arg in args.iter().rev() {
                    self.lower_expr(arg)?;
                }
                self.lower_expr(base)?;
                self.code.push(Instr::Call(args.len() as u32));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_lower_empty() -> Result<()> {
        let code = NitrousCode::lower(NitrousAst::parse(r"")?)?;
        assert_eq!(code.code.len(), 0);
        Ok(())
    }
}
