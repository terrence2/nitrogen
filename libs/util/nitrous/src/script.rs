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
use script::StatementsParser;

use crate::ir::Stmt;
use anyhow::{bail, Result};
use regex::Regex;
use std::fmt;

/// Intermediate exposed for completions and other meta purposes.
pub struct ScriptAst {
    #[allow(clippy::vec_box)]
    stmts: Vec<Box<Stmt>>,
}

impl ScriptAst {
    pub fn compile(script: &str) -> Result<Self> {
        let re = Regex::new(r"(/\*([^*]|[\r\n]|(\*+([^*/]|[\r\n])))*\*+/)|(//.*)")?;
        let preprocessed = re.replace_all(script, "");

        Ok(match StatementsParser::new().parse(&preprocessed) {
            Ok(stmts) => Self { stmts },
            Err(e) => {
                println!("parse failure: {}\nin: {}", e, script);
                bail!(format!("parse failure: {}", e))
            }
        })
    }

    pub fn statements(&self) -> &[Box<Stmt>] {
        &self.stmts
    }

    pub fn statements_mut(&mut self) -> &mut [Box<Stmt>] {
        &mut self.stmts
    }
}

#[derive(Debug, Clone)]
pub struct Script {
    #[allow(clippy::vec_box)]
    stmts: Vec<Box<Stmt>>,
}

impl Script {
    pub fn compile(script: &str) -> Result<Self> {
        let ast = ScriptAst::compile(script)?;
        Ok(Self { stmts: ast.stmts })
    }

    pub fn statements(&self) -> &[Box<Stmt>] {
        &self.stmts
    }
}

impl fmt::Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for stmt in &self.stmts {
            write!(f, "{}", stmt)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ir::{Expr, Operator, Term};
    use anyhow::{anyhow, Result};
    use ordered_float::OrderedFloat;

    #[test]
    fn script_terms() -> Result<()> {
        assert!(StatementsParser::new().parse("22").is_ok());
        assert!(StatementsParser::new().parse("(22)").is_ok());
        assert!(StatementsParser::new().parse("((((22))))").is_ok());
        assert_eq!(
            StatementsParser::new().parse("((\"a\"))")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::String(
                "a".to_owned()
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("((\'a\'))")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::String(
                "a".to_owned()
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("+123.")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Float(
                OrderedFloat(123f64)
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("-123.")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Float(
                OrderedFloat(-123f64)
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("+0.123")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Float(
                OrderedFloat(0.123f64)
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("-0.123")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Float(
                OrderedFloat(-0.123f64)
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("123.123")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Float(
                OrderedFloat(123.123f64)
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("-123.123")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Float(
                OrderedFloat(-123.123f64)
            )))))]
        );
        assert_eq!(
            StatementsParser::new().parse("asdf")?,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Symbol(
                "asdf".into()
            )))))]
        );
        Ok(())
    }

    #[test]
    fn test_empty() -> Result<()> {
        let rv = StatementsParser::new().parse("")?;
        assert_eq!(rv, vec![]);

        let script = Script::compile("// hello\n")?;
        assert_eq!(script.stmts, vec![]);

        Ok(())
    }

    #[test]
    fn test_expr() -> Result<()> {
        let rv = StatementsParser::new().parse("a + b * c")?;
        assert_eq!(
            rv,
            vec![Box::new(Stmt::Expr(Box::new(Expr::BinOp(
                Box::new(Expr::Term(Term::Symbol("a".to_owned()))),
                Operator::Add,
                Box::new(Expr::BinOp(
                    Box::new(Expr::Term(Term::Symbol("b".to_owned()))),
                    Operator::Multiply,
                    Box::new(Expr::Term(Term::Symbol("c".to_owned()))),
                )),
            ))))]
        );

        let script = Script::compile("foo.bar")?;
        assert_eq!(
            script.stmts,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Attr(
                Box::new(Expr::Term(Term::Symbol("foo".to_owned()))),
                Term::Symbol("bar".to_owned()),
            )))),]
        );

        let script = Script::compile("foo.bar()")?;
        assert_eq!(
            script.stmts,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Call(
                Box::new(Expr::Attr(
                    Box::new(Expr::Term(Term::Symbol("foo".to_owned()))),
                    Term::Symbol("bar".to_owned()),
                )),
                vec![]
            )))),]
        );

        let rv = StatementsParser::new().parse("foo.bar(a * 2, b)")?;
        assert_eq!(
            rv,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Call(
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
            ))))]
        );

        let rv = StatementsParser::new().parse("await foo.bar(a * 2, b)")?;
        assert_eq!(
            rv,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Await(Box::new(
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
            )))))]
        );

        let s = "a".to_owned();
        let rv = StatementsParser::new()
            .parse(&s)
            .map_err(|_| anyhow!("failed to parse expression"))?;
        assert_eq!(
            rv,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Symbol(
                "a".to_owned()
            )))))]
        );

        Ok(())
    }

    #[test]
    fn script_mismatched_parens() {
        assert!(script::StatementsParser::new().parse("((22)").is_err());
    }

    #[test]
    fn script_stmts() -> Result<()> {
        let rv = StatementsParser::new().parse(
            r#"
                2;
                3;
                4
            "#,
        )?;
        assert_eq!(
            *rv,
            vec![
                Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Integer(2))))),
                Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Integer(3))))),
                Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Integer(4))))),
            ]
        );
        Ok(())
    }

    #[test]
    fn script_single_line_comments() -> Result<()> {
        let rv = Script::compile(
            r#"
                // 2;
                3;
                // 4
            "#,
        )?;
        assert_eq!(
            rv.stmts,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Integer(3))))),]
        );
        Ok(())
    }

    #[test]
    fn script_multi_line_comments() -> Result<()> {
        let rv = Script::compile(
            r#"
                /* 2; */
                3;
                /*
                 * 4
                 */
            "#,
        )?;
        assert_eq!(
            rv.stmts,
            vec![Box::new(Stmt::Expr(Box::new(Expr::Term(Term::Integer(3))))),]
        );
        Ok(())
    }
}
