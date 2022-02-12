// MIT License
//
// Copyright (c) 2021 Jakob Hellermann
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

// http://www.gnu.org/licenses/license-list.html#Expat

use bevy_ecs::{component::ComponentId, prelude::*, schedule::SystemContainer};
use dot::DotGraph;
use pretty_type_name::pretty_type_name_str;
use std::{fs::File, io::Write, path::Path};

mod dot {
    use std::borrow::Cow;

    pub struct DotGraph {
        buffer: String,
    }

    fn escape_quote(input: &str) -> Cow<'_, str> {
        if input.contains('"') {
            Cow::Owned(input.replace('"', "\\\""))
        } else {
            Cow::Borrowed(input)
        }
    }

    fn escape_id(input: &str) -> Cow<'_, str> {
        if input.starts_with('<') && input.ends_with('>') {
            input.into()
        } else {
            format!("\"{}\"", escape_quote(input)).into()
        }
    }

    fn format_attributes(attrs: &[(&str, &str)]) -> String {
        let attrs: Vec<_> = attrs
            .iter()
            .map(|(a, b)| format!("{}={}", escape_id(a), escape_id(b)))
            .collect();
        let attrs = attrs.join(", ");
        format!("[{}]", attrs)
    }

    pub fn font_tag(text: &str, color: &str, size: u8) -> String {
        if text.is_empty() {
            return "".to_string();
        }
        format!(
            "<FONT COLOR=\"{}\" POINT-SIZE=\"{}\">{}</FONT>",
            color,
            size,
            html_escape(text)
        )
    }

    pub fn html_escape(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('\"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    impl DotGraph {
        fn new(name: &str, kind: &str, attrs: &[(&str, &str)]) -> DotGraph {
            let mut dot = DotGraph {
                buffer: String::new(),
            };

            dot.write(format!("{} {} {{", kind, escape_id(name)));

            for (key, val) in attrs {
                dot.write(format!("\t{}={};", escape_id(key), escape_id(val)));
            }

            dot
        }

        pub fn digraph(name: &str, options: &[(&str, &str)]) -> DotGraph {
            DotGraph::new(name, "digraph", options)
        }

        pub fn subgraph(name: &str, options: &[(&str, &str)]) -> DotGraph {
            DotGraph::new(&format!("cluster{}", name), "subgraph", options)
        }

        #[allow(dead_code)]
        pub fn graph_attributes(mut self, attrs: &[(&str, &str)]) -> Self {
            self.write(format!("\tgraph {};", format_attributes(attrs)));
            self
        }

        pub fn edge_attributes(mut self, attrs: &[(&str, &str)]) -> Self {
            self.write(format!("\tedge {};", format_attributes(attrs)));
            self
        }

        pub fn node_attributes(mut self, attrs: &[(&str, &str)]) -> Self {
            self.write(format!("\tnode {};", format_attributes(attrs)));
            self
        }

        #[allow(unused)]
        pub fn same_rank<I, S>(&mut self, nodes: I)
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            self.write_no_newline("{ rank = same;");
            for item in nodes {
                self.write(item);
                self.write("; ");
            }
            self.write("}");
        }

        pub fn finish(mut self) -> String {
            self.write("}");
            self.buffer
        }

        pub fn add_sub_graph(&mut self, graph: DotGraph) {
            let subgraph = graph.finish().replace('\n', "\n\t");
            self.write_no_newline("\t");
            self.write(subgraph);
        }

        /// label needs to include the quotes
        pub fn add_node(&mut self, id: &str, attrs: &[(&str, &str)]) {
            self.write(format!("\t{} {}", escape_id(id), format_attributes(attrs)));
        }
        pub fn add_invisible_node(&mut self, id: &str) {
            self.add_node(id, &[("style", "invis")]);
        }

        /// The DOT syntax actually allows subgraphs as the edge's nodes but this doesn't support it yet.
        pub fn add_edge(&mut self, from: &str, to: &str, attrs: &[(&str, &str)]) {
            self.add_edge_with_ports(from, None, to, None, attrs);
        }

        pub fn add_edge_with_ports(
            &mut self,
            from: &str,
            from_port: Option<&str>,
            to: &str,
            to_port: Option<&str>,
            attrs: &[(&str, &str)],
        ) {
            let from = if let Some(from_port) = from_port {
                format!("{}:{}", escape_id(from), escape_id(from_port))
            } else {
                escape_id(from).to_string()
            };
            let to = if let Some(to_port) = to_port {
                format!("{}:{}", escape_id(to), escape_id(to_port))
            } else {
                escape_id(to).to_string()
            };
            self.write(format!(
                "\t{} -> {} {}",
                &from,
                &to,
                format_attributes(attrs)
            ));
        }

        fn write_no_newline(&mut self, text: impl AsRef<str>) {
            self.buffer.push_str(text.as_ref());
        }

        fn write(&mut self, text: impl AsRef<str>) {
            self.buffer.push_str(text.as_ref());
            self.buffer.push('\n');
        }
    }
}

#[non_exhaustive]
pub struct SystemInfo<'a> {
    pub name: &'a str,
}

pub struct ScheduleGraphStyle {
    pub fontsize: f32,
    pub fontname: String,
    pub bgcolor: String,
    pub bgcolor_nested_schedule: String,
    pub bgcolor_stage: String,
    pub color_system: String,
    pub color_edge: String,
    pub hide_startup_schedule: bool,
    pub system_filter: Option<Box<dyn Fn(&SystemInfo) -> bool>>,
}
impl ScheduleGraphStyle {
    pub fn light() -> Self {
        ScheduleGraphStyle {
            fontsize: 16.0,
            fontname: "Helvetica".into(),
            bgcolor: "white".into(),
            bgcolor_nested_schedule: "#d1d5da".into(),
            bgcolor_stage: "#e1e5ea".into(),
            color_system: "white".into(),
            color_edge: "black".into(),
            hide_startup_schedule: true,
            system_filter: None,
        }
    }
    pub fn dark() -> Self {
        ScheduleGraphStyle {
            fontsize: 16.0,
            fontname: "Helvetica".into(),
            bgcolor: "#35393F".into(),
            bgcolor_nested_schedule: "#D0E1ED".into(),
            bgcolor_stage: "#99aab5".into(),
            color_system: "#eff1f3".into(),
            color_edge: "white".into(),
            hide_startup_schedule: true,
            system_filter: None,
        }
    }
}
impl Default for ScheduleGraphStyle {
    fn default() -> Self {
        ScheduleGraphStyle::dark()
    }
}

pub fn dump_schedule(world: &World, schedule: &Schedule, path: &Path) {
    let dot = schedule_graph_dot_styled_inner(world, schedule, None, &ScheduleGraphStyle::light());
    let mut fp = File::create(path).unwrap();
    fp.write_all(dot.as_bytes()).unwrap();
}

pub fn schedule_graph_dot_styled_inner(
    world: &World,
    schedule: &Schedule,
    use_world_info_for_stages: Option<(&World, &[&dyn StageLabel])>,
    style: &ScheduleGraphStyle,
) -> String {
    let mut graph = DotGraph::digraph(
        "schedule",
        &[
            ("fontsize", &style.fontsize.to_string()),
            ("fontname", &style.fontname),
            ("rankdir", "LR"),
            ("nodesep", "0.05"),
            ("bgcolor", &style.bgcolor),
            ("compound", "true"),
        ],
    )
    .node_attributes(&[("shape", "box"), ("margin", "0"), ("height", "0.4")])
    .edge_attributes(&[("color", &style.color_edge)]);

    build_schedule_graph(
        &mut graph,
        world,
        schedule,
        "schedule",
        None,
        use_world_info_for_stages,
        style,
    );

    graph.finish()
}

fn build_schedule_graph(
    graph: &mut DotGraph,
    world: &World,
    schedule: &Schedule,
    schedule_name: &str,
    marker_node_id: Option<&str>,
    use_world_info_for_stages: Option<(&World, &[&dyn StageLabel])>,
    style: &ScheduleGraphStyle,
) {
    if let Some(marker_id) = marker_node_id {
        graph.add_invisible_node(marker_id);
    }

    let is_startup_schedule =
        |stage_name: &dyn StageLabel| format!("{:?}", stage_name) == "Startup";

    for (stage_name, stage) in schedule.iter_stages() {
        if let Some(system_stage) = stage.downcast_ref::<SystemStage>() {
            let subgraph = system_stage_subgraph(
                world,
                schedule_name,
                stage_name,
                system_stage,
                use_world_info_for_stages,
                style,
            );
            graph.add_sub_graph(subgraph);
        } else if let Some(schedule) = stage.downcast_ref::<Schedule>() {
            if style.hide_startup_schedule && is_startup_schedule(stage_name) {
                continue;
            }

            let name = format!("cluster_{:?}", stage_name);

            let marker_id = marker_id(schedule_name, stage_name);
            let stage_name_str = format!("{:?}", stage_name);

            let mut schedule_sub_graph = DotGraph::subgraph(
                &name,
                &[
                    ("label", &stage_name_str),
                    ("fontsize", "20"),
                    ("constraint", "false"),
                    ("rankdir", "LR"),
                    ("style", "rounded"),
                    ("bgcolor", &style.bgcolor_nested_schedule),
                ],
            )
            .edge_attributes(&[("color", &style.color_edge)]);
            build_schedule_graph(
                &mut schedule_sub_graph,
                world,
                schedule,
                &name,
                Some(&marker_id),
                use_world_info_for_stages,
                style,
            );
            graph.add_sub_graph(schedule_sub_graph);
        } else {
            eprintln!("Missing downcast: {:?}", stage_name);
        }
    }

    let iter_a = schedule
        .iter_stages()
        .filter(|(stage, _)| !style.hide_startup_schedule || !is_startup_schedule(*stage));
    let iter_b = schedule
        .iter_stages()
        .filter(|(stage, _)| !style.hide_startup_schedule || !is_startup_schedule(*stage))
        .skip(1);

    for ((a, _), (b, _)) in iter_a.zip(iter_b) {
        let a = marker_id(schedule_name, a);
        let b = marker_id(schedule_name, b);
        graph.add_edge(&a, &b, &[]);
    }
}

fn marker_id(schedule_name: &str, stage_name: &dyn StageLabel) -> String {
    format!("MARKER_{}_{:?}", schedule_name, stage_name,)
}

fn system_stage_subgraph(
    world: &World,
    schedule_name: &str,
    stage_name: &dyn StageLabel,
    system_stage: &SystemStage,
    use_world_info_for_stages: Option<(&World, &[&dyn StageLabel])>,
    style: &ScheduleGraphStyle,
) -> DotGraph {
    let stage_name_str = format!("{:?}", stage_name);

    let mut sub = DotGraph::subgraph(
        &format!("cluster_{:?}", stage_name),
        &[
            ("style", "rounded"),
            ("color", &style.bgcolor_stage),
            ("bgcolor", &style.bgcolor_stage),
            ("rankdir", "TD"),
            ("label", &stage_name_str),
        ],
    )
    .node_attributes(&[
        ("style", "filled"),
        ("color", &style.color_system),
        ("bgcolor", &style.color_system),
    ]);

    sub.add_invisible_node(&marker_id(schedule_name, stage_name));

    let relevant_world = match use_world_info_for_stages {
        Some((relevant_world, stages)) if stages.contains(&stage_name) => relevant_world,
        _ => world,
    };

    add_systems_to_graph(
        &mut sub,
        relevant_world,
        schedule_name,
        SystemKind::ExclusiveStart,
        system_stage.exclusive_at_start_systems(),
        style,
    );
    add_systems_to_graph(
        &mut sub,
        relevant_world,
        schedule_name,
        SystemKind::ExclusiveBeforeCommands,
        system_stage.exclusive_before_commands_systems(),
        style,
    );
    add_systems_to_graph(
        &mut sub,
        relevant_world,
        schedule_name,
        SystemKind::Parallel,
        system_stage.parallel_systems(),
        style,
    );
    add_systems_to_graph(
        &mut sub,
        relevant_world,
        schedule_name,
        SystemKind::ExclusiveEnd,
        system_stage.exclusive_at_end_systems(),
        style,
    );

    sub
}

enum SystemKind {
    ExclusiveStart,
    ExclusiveEnd,
    ExclusiveBeforeCommands,
    Parallel,
}
fn add_systems_to_graph<T: SystemContainer>(
    graph: &mut DotGraph,
    world: &World,
    schedule_name: &str,
    kind: SystemKind,
    systems: &[T],
    style: &ScheduleGraphStyle,
) {
    let mut systems: Vec<_> = systems.iter().collect();
    systems.sort_by_key(|system| system.name());

    if systems.is_empty() {
        return;
    }

    for (i, &system_container) in systems.iter().enumerate() {
        let id = node_id(schedule_name, system_container, i);
        let system_name = system_container.name();

        if let Some(filter) = &style.system_filter {
            let info = SystemInfo {
                name: system_name.as_ref(),
            };
            if !filter(&info) {
                continue;
            }
        }

        let short_system_name = pretty_type_name_str(&system_container.name());

        let kind = match kind {
            SystemKind::ExclusiveStart => Some("Exclusive at start"),
            SystemKind::ExclusiveEnd => Some("Exclusive at end"),
            SystemKind::ExclusiveBeforeCommands => Some("Exclusive before commands"),
            SystemKind::Parallel => None,
        };

        let label = match kind {
            Some(kind) => {
                format!(
                    r#"<{}<BR />{}>"#,
                    &dot::html_escape(&short_system_name),
                    dot::font_tag(kind, "red", 11),
                )
            }
            None => short_system_name,
        };

        let tooltip = system_tooltip(system_container, world);
        graph.add_node(&id, &[("label", &label), ("tooltip", &tooltip)]);

        add_dependency_labels(
            graph,
            schedule_name,
            &id,
            SystemDirection::Before,
            system_container.before(),
            &systems,
        );
        add_dependency_labels(
            graph,
            schedule_name,
            &id,
            SystemDirection::After,
            system_container.after(),
            &systems,
        );
    }
}

fn system_tooltip<T: SystemContainer>(system_container: &T, world: &World) -> String {
    let mut tooltip = String::new();
    let truncate_in_place =
        |tooltip: &mut String, end: &str| tooltip.truncate(tooltip.trim_end_matches(end).len());

    let components = world.components();
    let name_of_component = |id| {
        pretty_type_name_str(
            components
                .get_info(id)
                .map_or_else(|| "<missing>", |info| info.name()),
        )
    };

    let is_resource = |id: &ComponentId| world.archetypes().resource().contains(*id);

    if let Some(component_access) = system_container.component_access() {
        let (read_resources, read_components): (Vec<_>, Vec<_>) =
            component_access.reads().partition(is_resource);
        let (write_resources, write_components): (Vec<_>, Vec<_>) =
            component_access.writes().partition(is_resource);

        let mut list = |name, components: &[ComponentId]| {
            if components.is_empty() {
                return;
            }
            tooltip.push_str(name);
            tooltip.push_str(" [");
            for read_resource in components {
                tooltip.push_str(&name_of_component(*read_resource));
                tooltip.push_str(", ");
            }
            truncate_in_place(&mut tooltip, ", ");
            tooltip.push_str("]\\n");
        };

        list("Components", &read_components);
        list("ComponentsMut", &write_components);

        list("Res", &read_resources);
        list("ResMut", &write_resources);
    }

    if tooltip.is_empty() {
        pretty_type_name_str(&system_container.name())
    } else {
        tooltip
    }
}

enum SystemDirection {
    Before,
    After,
}
fn add_dependency_labels(
    graph: &mut DotGraph,
    schedule_name: &str,
    system_node_id: &str,
    direction: SystemDirection,
    requirements: &[Box<dyn SystemLabel>],
    other_systems: &[&impl SystemContainer],
) {
    for requirement in requirements {
        let mut found = false;
        for (i, &dependency) in other_systems
            .iter()
            .enumerate()
            .filter(|(_, node)| node.labels().contains(requirement))
        {
            found = true;

            let me = system_node_id;
            let other = node_id(schedule_name, dependency, i);

            match direction {
                SystemDirection::Before => graph.add_edge(me, &other, &[("constraint", "false")]),
                SystemDirection::After => graph.add_edge(&other, me, &[("constraint", "false")]),
            }
        }
        assert!(found);
    }
}

fn node_id(schedule_name: &str, system: &impl SystemContainer, i: usize) -> String {
    format!("{}_{}_{}", schedule_name, system.name(), i)
}
