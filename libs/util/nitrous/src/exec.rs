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
use crate::{lower::Instr, LocalNamespace, NitrousScript, Value, WorldIndex};
use anyhow::{anyhow, bail, Result};
use bevy_ecs::prelude::*;

/// Store current execution state of some specific script.
/// Note: this state must always be used with the same script.
pub struct ExecutionContext {
    locals: LocalNamespace,
    stack: Vec<Value>,
    script: NitrousScript,
    counter: usize,
}

impl ExecutionContext {
    pub fn new(locals: LocalNamespace, script: NitrousScript) -> Self {
        Self {
            locals,
            stack: Vec::new(),
            script,
            counter: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum YieldState {
    Yielded,
    Finished(Value),
}

/// Executing scripts requires some state and some
pub struct NitrousExecutor<'a> {
    state: &'a mut ExecutionContext,
    index: &'a mut WorldIndex,
    world: &'a mut World,
}

impl<'a> NitrousExecutor<'a> {
    pub fn new(
        state: &'a mut ExecutionContext,
        index: &'a mut WorldIndex,
        world: &'a mut World,
    ) -> Self {
        Self {
            state,
            index,
            world,
        }
    }

    fn push(&mut self, value: Value) {
        self.state.stack.push(value);
    }

    fn pop(&mut self, ctx: &str) -> Result<Value> {
        self.state
            .stack
            .pop()
            .ok_or_else(|| anyhow!("empty stack at pop: {}", ctx))
    }

    pub fn run_until_yield(&mut self) -> Result<YieldState> {
        for pc in self.state.counter..self.state.script.code().len() {
            let instr = self.state.script.code()[pc].to_owned();
            println!("AT: {:?}", instr);
            match instr {
                Instr::Push(value) => self.state.stack.push(value.to_owned()),
                Instr::LoadLocalOrResource(atom) => {
                    let name = self.state.script.atom(&atom);
                    if let Some(value) = self.state.locals.get(name) {
                        self.push(value);
                    } else if let Some(resource) = self.index.lookup_resource(name) {
                        self.push(resource);
                    } else {
                        bail!("unknown local or resource varable: {}", name);
                    }
                }
                Instr::LoadEntity(atom) => {
                    let name = self.state.script.atom(&atom);
                    let entity = self
                        .index
                        .lookup_entity(name)
                        .ok_or_else(|| anyhow!("no such entity: @{}", name))?;
                    self.push(entity);
                }
                Instr::InitLocal(atom) => {
                    let value = self.pop("assigned")?;
                    let target = self.state.script.atom(&atom);
                    self.state.locals.put(target, value);
                }
                Instr::StoreLocal(atom) => {
                    let value = self.pop("assigned")?;
                    let target = self.state.script.atom(&atom);
                    self.state.locals.put(target, value);
                }

                Instr::Multiply => {
                    let rhs = self.pop("rhs")?;
                    let lhs = self.pop("lhs")?;
                    self.push(lhs.impl_multiply(rhs)?);
                }
                Instr::Divide => {
                    let rhs = self.pop("rhs")?;
                    let lhs = self.pop("lhs")?;
                    self.push(lhs.impl_divide(rhs)?);
                }
                Instr::Add => {
                    let rhs = self.pop("rhs")?;
                    let lhs = self.pop("lhs")?;
                    self.push(lhs.impl_add(rhs)?);
                }
                Instr::Subtract => {
                    let rhs = self.pop("rhs")?;
                    let lhs = self.pop("lhs")?;
                    self.push(lhs.impl_subtract(rhs)?);
                }
                Instr::Call(arg_cnt) => {
                    let base = self.pop("call target")?;
                    let mut args = Vec::with_capacity(arg_cnt as usize);
                    for _ in 0..arg_cnt {
                        args.push(self.pop("arg")?);
                    }

                    match base {
                        Value::ResourceMethod(mut resource, method_name) => {
                            let result = resource.call_method(&method_name, &args)?;
                            self.push(result);
                        }
                        _ => bail!("call must be on a method value"),
                    }
                }
                Instr::Attr(atom) => {
                    let base = self.pop("attr base")?;
                    let result = base.attr(self.state.script.atom(&atom))?;
                    self.push(result);
                }
                Instr::Await => {
                    unimplemented!()
                }
            }
        }
        println!("STACK AT EXIT: {:#?}", self.state.stack);
        Ok(YieldState::Finished(self.pop("return value")?))
    }
}
