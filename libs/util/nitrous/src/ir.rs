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
use ordered_float::OrderedFloat;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stmt {
    LetAssign(Term, Box<Expr>),
    Expr(Box<Expr>),
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LetAssign(term, expr) => write!(f, "let {} := {};", term, expr),
            Self::Expr(expr) => write!(f, "{};", expr),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expr {
    Attr(Box<Expr>, Term),
    Await(Box<Expr>),
    #[allow(clippy::vec_box)]
    Call(Box<Expr>, Vec<Box<Expr>>),
    BinOp(Box<Expr>, Operator, Box<Expr>),
    Assign(Term, Box<Expr>),
    AssignAttr(Box<Expr>, Term, Box<Expr>),
    Term(Term),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attr(b, n) => write!(f, "{}.{}", b, n),
            Self::Await(e) => write!(f, "await {}", e),
            Self::Call(func, args) => {
                write!(f, "{}(", func)?;
                for (i, a) in args.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", a)?;
                }
                write!(f, ")")
            }
            Self::BinOp(a, op, b) => write!(f, "{} {} {}", a, op, b),
            Self::Assign(t, e) => write!(f, "{} := {}", t, e),
            Self::AssignAttr(t, n, e) => write!(f, "{}.{} := {}", t, n, e),
            Self::Term(t) => write!(f, "{}", t),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Term {
    AtSymbol(String),
    Symbol(String),
    Boolean(bool),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AtSymbol(v) => write!(f, "@{}", v),
            Self::Symbol(v) => write!(f, "{}", v),
            Self::Boolean(b) => {
                if *b {
                    write!(f, "True")
                } else {
                    write!(f, "False")
                }
            }
            Self::Integer(v) => write!(f, "{}", v),
            Self::Float(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "\"{}\"", v),
        }
    }
}
