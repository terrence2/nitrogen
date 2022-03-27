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
use nitrous::{inject_nitrous_resource, NitrousResource};
use runtime::{Extension, Runtime};
use structopt::StructOpt;
use tracing_subscriber::{
    fmt::{format::DefaultFields, FormattedFields},
    prelude::*,
    registry::Registry,
};

// Inspired heavily by bevy_log

#[derive(Clone, Debug, StructOpt)]
pub struct TraceLogOpts {
    /// Capture a chrome-format execution trace.
    #[structopt(short = "T", long)]
    trace: bool,
}

// Enable logging of traces
#[derive(Debug, NitrousResource)]
pub struct TraceLog;

#[inject_nitrous_resource]
impl TraceLog {}

impl Extension for TraceLog {
    fn init(runtime: &mut Runtime) -> Result<()> {
        if let Some(opts) = runtime.maybe_resource::<TraceLogOpts>() {
            if !opts.trace {
                return Ok(());
            }

            let subscriber = Registry::default();
            let subscriber = subscriber.with(tracing_error::ErrorLayer::default());
            let chrome_layer = {
                let (chrome_layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
                    .name_fn(Box::new(|event_or_span| match event_or_span {
                        tracing_chrome::EventOrSpan::Event(event) => event.metadata().name().into(),
                        tracing_chrome::EventOrSpan::Span(span) => {
                            if let Some(fields) =
                                span.extensions().get::<FormattedFields<DefaultFields>>()
                            {
                                format!("{}: {}", span.metadata().name(), fields.fields.as_str())
                            } else {
                                span.metadata().name().into()
                            }
                        }
                    }))
                    .build();
                runtime.insert_non_send(guard);
                chrome_layer
            };

            let fmt_layer = tracing_subscriber::fmt::Layer::default();
            let subscriber = subscriber.with(fmt_layer);
            let subscriber = subscriber.with(chrome_layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("Could not set global default tracing subscriber. If you've already set up a tracing subscriber, please disable LogPlugin from Bevy's DefaultPlugins");
        }

        Ok(())
    }
    /* Bevy tracing fragments for wasm and android

       #[cfg(target_arch = "wasm32")]
           {
               console_error_panic_hook::set_once();
               let subscriber = subscriber.with(tracing_wasm::WASMLayer::new(
                   tracing_wasm::WASMLayerConfig::default(),
               ));
               bevy_utils::tracing::subscriber::set_global_default(subscriber)
                   .expect("Could not set global default tracing subscriber. If you've already set up a tracing subscriber, please disable LogPlugin from Bevy's DefaultPlugins");
           }

       #[cfg(target_os = "android")]
           {
               let subscriber = subscriber.with(android_tracing::AndroidLayer::default());
               bevy_utils::tracing::subscriber::set_global_default(subscriber)
                   .expect("Could not set global default tracing subscriber. If you've already set up a tracing subscriber, please disable LogPlugin from Bevy's DefaultPlugins");
           }
    */
}
