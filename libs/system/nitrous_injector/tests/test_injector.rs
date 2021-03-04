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
use failure::Fallible;
use nitrous::{Interpreter, Module, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug, NitrousModule)]
struct TestInjector {}

#[inject_nitrous_module]
impl TestInjector {
    #[method]
    fn plain(&self) {
        println!("Called Plain");
    }

    #[method]
    fn boolean(&self, b: bool) -> bool {
        b
    }

    #[method]
    fn integer(&self, i: i64) -> i64 {
        i * 2
    }

    #[method]
    fn float(&self, f: f64) -> f64 {
        f * 2.
    }

    #[method]
    fn string(&self, s: &str) -> String {
        s.to_owned() + ", world!"
    }

    #[method]
    fn value(&self, v: Value) -> Value {
        v
    }

    #[method]
    fn fail_plain(&self) -> Fallible<()> {
        println!("Called Fail Plain");
        Ok(())
    }

    #[method]
    fn fail_boolean(&self, b: bool) -> Fallible<bool> {
        Ok(b)
    }

    #[method]
    fn fail_integer(&self, i: i64) -> Fallible<i64> {
        Ok(i * 2)
    }

    #[method]
    fn fail_float(&self, f: f64) -> Fallible<f64> {
        Ok(f * 2.)
    }

    #[method]
    fn fail_string(&self, s: &str) -> Fallible<String> {
        Ok(s.to_owned() + ", world!")
    }

    #[method]
    fn fail_value(&self, v: Value) -> Fallible<Value> {
        Ok(v)
    }
}

#[test]
fn test_it_works() -> Fallible<()> {
    let interpreter = Interpreter::default().init()?;
    let inj = Arc::new(RwLock::new(TestInjector {}));
    interpreter
        .write()
        .put(interpreter.clone(), "test", Value::Module(inj))?;

    assert_eq!(
        interpreter.write().interpret_once("test.plain()")?,
        Value::True()
    );
    assert_eq!(
        interpreter.write().interpret_once("test.boolean(True)")?,
        Value::True()
    );
    assert_eq!(
        interpreter.write().interpret_once("test.integer(42)")?,
        Value::Integer(84)
    );
    assert_eq!(
        interpreter.write().interpret_once("test.float(42.0)")?,
        Value::Float(OrderedFloat(84.0))
    );
    assert_eq!(
        interpreter
            .write()
            .interpret_once(r#"test.string("hello")"#)?,
        Value::String("hello, world!".to_string())
    );
    assert_eq!(
        interpreter.write().interpret_once(r#"test.value(2)"#)?,
        Value::Integer(2)
    );

    // Fallible versions
    assert_eq!(
        interpreter
            .write()
            .interpret_once("test.fail_boolean(True)")?,
        Value::True()
    );
    assert_eq!(
        interpreter
            .write()
            .interpret_once("test.fail_integer(42)")?,
        Value::Integer(84)
    );
    assert_eq!(
        interpreter
            .write()
            .interpret_once(r#"test.fail_string("hello")"#)?,
        Value::String("hello, world!".to_string())
    );
    assert_eq!(
        interpreter
            .write()
            .interpret_once(r#"test.fail_value(2)"#)?,
        Value::Integer(2)
    );

    Ok(())
}
