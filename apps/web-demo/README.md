## Nitrogen Web Demo

The wasm32 target is not quite transparent yet:
* we cannot infinite loop
* we cannot run from a background thread
* tokio Runtime panics when created
* webgpu cannot do uploads
* must be compiled from lib.rs, not main.rs

We might be able to share code by having a dual lib/app crate eventually, but the gap is currently too large to bother.

Run with:
```shell script
RUSTFLAGS="--cfg=web_sys_unstable_apis" npm run serve
```
