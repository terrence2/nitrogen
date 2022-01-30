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
    lower::{Atom, Instr, NitrousCode},
};
use anyhow::Result;
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone)]
pub struct NitrousScript {
    code: Vec<Instr>,
    atoms: HashMap<Atom, String>,
}

impl NitrousScript {
    pub fn compile(script: &str) -> Result<Self> {
        let ast = NitrousAst::parse(script)?;
        let (code, atoms) = NitrousCode::lower(ast)?.finish()?;
        Ok(Self { code, atoms })
    }

    pub fn code(&self) -> &[Instr] {
        &self.code
    }

    pub fn atom(&self, atom: &Atom) -> &str {
        &self.atoms[atom]
    }

    pub fn atoms(&self) -> &HashMap<Atom, String> {
        &self.atoms
    }
}

impl From<&NitrousScript> for NitrousScript {
    fn from(script_ref: &NitrousScript) -> Self {
        script_ref.to_owned()
    }
}

impl fmt::Display for NitrousScript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, instr) in self.code.iter().enumerate() {
            match instr {
                Instr::Push(v) => writeln!(f, "{:03} <-- {}", i, v)?,
                Instr::LoadLocalOrResource(atom) => {
                    writeln!(f, "{:03} ==> {}", i, self.atoms.get(atom).unwrap())?
                }
                Instr::LoadEntity(atom) => {
                    writeln!(f, "{:03} ==> @{}", i, self.atoms.get(atom).unwrap())?
                }
                Instr::InitLocal(atom) => {
                    writeln!(f, "{:03} <== {}", i, &self.atoms.get(atom).unwrap())?
                }
                Instr::StoreLocal(atom) => {
                    writeln!(f, "{:03} <-- {}", i, &self.atoms.get(atom).unwrap())?
                }
                Instr::Multiply => writeln!(f, "{:03} <-> Multiply", i)?,
                Instr::Divide => writeln!(f, "{:03} <-> Divide", i)?,
                Instr::Add => writeln!(f, "{:03} <-> Add", i)?,
                Instr::Subtract => writeln!(f, "{:03} <-> Subtract", i)?,
                Instr::Call(cnt) => writeln!(f, "{:03} <-> Call({})", i, cnt)?,
                Instr::Attr(atom) => {
                    writeln!(f, "{:03} <-> .{}", i, &self.atoms.get(atom).unwrap())?
                }
                Instr::Await => writeln!(f, "{:03} <-> Await", i)?,
            }
        }
        Ok(())
    }
}
