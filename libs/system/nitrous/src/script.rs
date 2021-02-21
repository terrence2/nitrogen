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
use lalrpop_util::lalrpop_mod;
lalrpop_mod!(#[allow(clippy::all)] pub(crate) script);
use script::ExprParser;

use crate::ir::Expr;
use failure::{bail, Fallible};

pub struct Script {
    pub(crate) expr: Box<Expr>,
}

impl Script {
    pub fn compile_expr(raw: &str) -> Fallible<Self> {
        Ok(match ExprParser::new().parse(raw) {
            Ok(expr) => Self { expr },
            Err(e) => {
                println!("parse failure: {}", e);
                bail!(format!("parse failure: {}", e))
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ir::{Expr, Operator, Term};
    use failure::{err_msg, Fallible};
    use ordered_float::OrderedFloat;

    #[test]
    fn script_terms() -> Fallible<()> {
        assert!(ExprParser::new().parse("22").is_ok());
        assert!(ExprParser::new().parse("(22)").is_ok());
        assert!(ExprParser::new().parse("((((22))))").is_ok());
        assert_eq!(
            ExprParser::new().parse("((\"a\"))")?,
            Box::new(Expr::Term(Term::String("a".to_owned())))
        );
        assert_eq!(
            ExprParser::new().parse("((\'a\'))")?,
            Box::new(Expr::Term(Term::String("a".to_owned())))
        );
        assert_eq!(
            ExprParser::new().parse("+123.")?,
            Box::new(Expr::Term(Term::Float(OrderedFloat(123f64))))
        );
        assert_eq!(
            ExprParser::new().parse("-123.")?,
            Box::new(Expr::Term(Term::Float(OrderedFloat(-123f64))))
        );
        assert_eq!(
            ExprParser::new().parse("+0.123")?,
            Box::new(Expr::Term(Term::Float(OrderedFloat(0.123f64))))
        );
        assert_eq!(
            ExprParser::new().parse("-0.123")?,
            Box::new(Expr::Term(Term::Float(OrderedFloat(-0.123f64))))
        );
        assert_eq!(
            ExprParser::new().parse("123.123")?,
            Box::new(Expr::Term(Term::Float(OrderedFloat(123.123f64))))
        );
        assert_eq!(
            ExprParser::new().parse("-123.123")?,
            Box::new(Expr::Term(Term::Float(OrderedFloat(-123.123f64))))
        );
        assert_eq!(
            ExprParser::new().parse("asdf")?,
            Box::new(Expr::Term(Term::Symbol("asdf".into())))
        );
        Ok(())
    }

    #[test]
    fn test_expr() -> Fallible<()> {
        let rv = ExprParser::new().parse("a + b * c")?;
        assert_eq!(
            *rv,
            Expr::BinOp(
                Box::new(Expr::BinOp(
                    Box::new(Expr::Term(Term::Symbol("a".to_owned()))),
                    Operator::Add,
                    Box::new(Expr::Term(Term::Symbol("b".to_owned()))),
                )),
                Operator::Multiply,
                Box::new(Expr::Term(Term::Symbol("c".to_owned()))),
            )
        );

        let rv = ExprParser::new().parse("foo.bar(a * 2, b)")?;
        assert_eq!(
            *rv,
            Expr::Call(
                Box::new(Expr::Attr(
                    Box::new(Expr::Term(Term::Symbol("foo".to_owned()))),
                    Term::Symbol("bar".to_owned()),
                )),
                vec![
                    Box::new(Expr::BinOp(
                        Box::new(Expr::Term(Term::Symbol("a".to_owned()))),
                        Operator::Multiply,
                        Box::new(Expr::Term(Term::Integer(2))),
                    )),
                    Box::new(Expr::Term(Term::Symbol("b".to_owned()))),
                ]
            )
        );

        let s = "a".to_owned();
        let rv = ExprParser::new()
            .parse(&s)
            .map_err(|_| err_msg("failed to parse expression"))?;
        assert_eq!(*rv, Expr::Term(Term::Symbol("a".to_owned())));

        Ok(())
    }

    #[test]
    fn script_mismatched_parens() {
        assert!(script::ExprParser::new().parse("((22)").is_err());
    }
}
