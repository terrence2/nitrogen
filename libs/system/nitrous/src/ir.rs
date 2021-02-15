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

#[derive(Debug, Eq, PartialEq)]
pub enum Expr {
    BinOp(Box<Expr>, Operator, Box<Expr>),
    Term(Term),
    Attr(Box<Expr>, Term),
    Call(Box<Expr>, Vec<Box<Expr>>),
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Term {
    Symbol(String),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
}
