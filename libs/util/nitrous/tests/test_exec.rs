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
use bevy_ecs::{prelude::*, system::Resource};
use nitrous::{
    inject_nitrous, method, ExecutionContext, LocalNamespace, NitrousExecutor, NitrousResource,
    NitrousScript, ScriptResource, Value, WorldIndex, YieldState,
};

struct Test {
    context: ExecutionContext,
    world: World,
    index: WorldIndex,
}

impl Test {
    fn executor(&mut self) -> NitrousExecutor {
        NitrousExecutor::new(&mut self.context, &mut self.index, &mut self.world)
    }

    fn spawn_named_entity<T>(&mut self, name: &str)
    where
        T: ScriptEntity + 'static,
    {
        // let ent = RtEntity::self.world.spawn();
        // let name = name.to_owned();
        // let resource = self.world.get_resource::<T>().unwrap();
        // self.index.insert_named_resource(name, resource);
        unimplemented!()
    }

    fn insert_named_resource<T>(&mut self, name: &str, value: T)
    where
        T: Resource + ScriptResource + 'static,
    {
        self.world.insert_resource(value);
        let name = name.to_owned();
        let resource = self.world.get_resource::<T>().unwrap();
        self.index.insert_named_resource(name, resource);
    }

    fn compile(s: &str) -> Result<Self> {
        Ok(Self {
            context: ExecutionContext::new(LocalNamespace::empty(), NitrousScript::compile(s)?),
            world: World::default(),
            index: WorldIndex::empty(),
        })
    }

    fn exec(s: &str) -> Result<Value> {
        Self::compile(s)?.run()
    }

    fn run(&mut self) -> Result<Value> {
        match self.executor().run_until_yield()? {
            YieldState::Yielded => unimplemented!(),
            YieldState::Finished(v) => Ok(v),
        }
    }
}

#[test]
fn test_basic() -> Result<()> {
    assert_eq!(Test::exec("2 + 2")?, Value::Integer(4));
    Ok(())
}

#[derive(Debug, NitrousResource)]
struct TestResource {
    count: i64,
}

#[inject_nitrous]
impl TestResource {
    #[method]
    fn increment(&mut self) {
        self.count += 1;
    }

    #[method]
    fn value(&self) -> i64 {
        self.count
    }
}

#[test]
fn test_resource() -> Result<()> {
    let mut test = Test::compile("res.increment(); res.value();")?;
    test.insert_named_resource("res", TestResource { count: 2 });
    assert_eq!(test.run()?, Value::Integer(3));
    Ok(())
}

#[derive(Debug, Component, NitrousResource)]
struct MyPosition {
    x: i64,
}

#[inject_nitrous]
impl MyPosition {
    #[method]
    fn move_right(&mut self) {
        self.x += 1;
    }

    #[method]
    fn move_left(&mut self) {
        self.x -= 1;
    }

    #[method]
    fn value(&self) -> i64 {
        self.x
    }
}

#[test]
fn test_entity() -> Result<()> {
    let mut test = Test::compile("@position.move_right(); @position.value();")?;
    // test.spawn_named_entity("position")
    //     .insert(MyPosition { x: 2 });
    assert_eq!(test.run()?, Value::Integer(3));
    Ok(())
}
