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
pub enum Expr {
    BinOp(Box<Expr>, Operator, Box<Expr>),
    Term(Term),
    Attr(Box<Expr>, Term),
    #[allow(clippy::vec_box)]
    Call(Box<Expr>, Vec<Box<Expr>>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinOp(a, op, b) => write!(f, "{} {} {}", a, op, b),
            Self::Term(t) => write!(f, "{}", t),
            Self::Attr(b, n) => write!(f, "{}.{}", b, n),
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
    Symbol(String),
    Boolean(bool),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
