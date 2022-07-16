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
use anyhow::Result;
use bevy_ecs::prelude::*;
use nitrous::{
    inject_nitrous_component, inject_nitrous_resource, method, NitrousComponent, NitrousResource,
    Value,
};
use runtime::{Runtime, ScriptCompletions, ScriptHerder, ScriptRunPhase};
use std::collections::HashMap;

#[derive(Debug, NitrousResource)]
pub struct Globals {
    #[property]
    bool_resource: bool,

    #[property]
    int_resource: i64,

    #[property]
    float_resource: f64,

    #[property]
    string_resource: String,
}

#[inject_nitrous_resource]
impl Globals {
    pub fn new() -> Self {
        Self {
            bool_resource: true,
            int_resource: 42_i64,
            float_resource: 42_f64,
            string_resource: "Foobar".to_owned(),
        }
    }

    #[method]
    fn add_float(&self, v: f64) -> f64 {
        self.float_resource + v
    }

    #[method]
    fn add_int(&self, v: i64) -> i64 {
        self.int_resource + v
    }

    #[method]
    fn add_string(&self, v: &str) -> String {
        self.string_resource.clone() + v
    }

    #[method]
    fn check_bool(&self, v: bool) -> bool {
        self.bool_resource == v
    }
}

#[derive(Debug, Component, NitrousComponent)]
#[Name = "item"]
pub struct Item {
    #[property]
    float_resource: f64,
}

#[inject_nitrous_component]
impl Item {
    pub fn new() -> Self {
        Self {
            float_resource: 42_f64,
        }
    }

    #[method]
    fn add_float(&self, v: f64) -> f64 {
        self.float_resource + v
    }
}

#[test]
fn integration_test() -> Result<()> {
    env_logger::init();

    let mut runtime = Runtime::default();
    runtime.insert_named_resource("globals", Globals::new());
    runtime.spawn_named("player")?.insert_named(Item::new())?;

    // Resource
    let br0 = runtime.run_string("globals.bool_resource")?;
    let br1 = runtime.run_string("globals.bool_resource := False")?;
    let br2 = runtime.run_string("globals.check_bool(False)")?;

    let ir0 = runtime.run_string("globals.int_resource")?;
    let ir1 = runtime.run_string("globals.int_resource := 2")?;
    let ir2 = runtime.run_string("globals.add_int(2)")?;

    let fr0 = runtime.run_string("globals.float_resource")?;
    let fr1 = runtime.run_string("globals.float_resource := 2.")?;
    let fr2 = runtime.run_string("globals.add_float(2.)")?;

    let sr0 = runtime.run_string("globals.string_resource")?;
    let sr1 = runtime.run_string("globals.string_resource := \"Hello\"")?;
    let sr2 = runtime.run_string("globals.add_string(\", World!\")")?;

    // Entity
    let fe0 = runtime.run_string("@player.item.float_resource")?;
    let fe1 = runtime.run_string("@player.item.float_resource := 2.")?;
    let fe2 = runtime.run_string("@player.item.add_float(2.)")?;

    runtime.resource_scope(|heap, mut herder: Mut<ScriptHerder>| {
        herder._run_scripts(heap, ScriptRunPhase::Startup);
    });

    let mut completions = HashMap::new();
    for completion in runtime.resource::<ScriptCompletions>() {
        completions.insert(completion.receipt, completion);
    }

    // Resource
    assert_eq!(completions[&br0].unwrap(), Value::from_bool(true));
    assert_eq!(completions[&br1].unwrap(), Value::True());
    assert_eq!(completions[&br2].unwrap(), Value::from_bool(true));

    assert_eq!(completions[&ir0].unwrap(), Value::from_int(42));
    assert_eq!(completions[&ir1].unwrap(), Value::True());
    assert_eq!(completions[&ir2].unwrap(), Value::from_int(4));

    assert_eq!(completions[&fr0].unwrap(), Value::from_float(42_f64));
    assert_eq!(completions[&fr1].unwrap(), Value::True());
    assert_eq!(completions[&fr2].unwrap(), Value::from_float(4_f64));

    assert_eq!(completions[&sr0].unwrap(), Value::from_str("Foobar"));
    assert_eq!(completions[&sr1].unwrap(), Value::True());
    assert_eq!(completions[&sr2].unwrap(), Value::from_str("Hello, World!"));

    // Entity
    assert_eq!(completions[&fr0].unwrap(), Value::from_float(42_f64));
    assert_eq!(completions[&fr1].unwrap(), Value::True());
    assert_eq!(completions[&fr2].unwrap(), Value::from_float(4_f64));

    Ok(())
}
