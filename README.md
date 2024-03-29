[![pipeline status](https://gitlab.com/terrence_too/nitrogen/badges/main/pipeline.svg)](https://gitlab.com/terrence_too/nitrogen/-/commits/main)
[![Latest
Release](https://gitlab.com/terrence_too/nitrogen/-/badges/release.svg)](https://gitlab.com/terrence_too/nitrogen/-/releases)

## Project Status
* Graphics / Input Engine
  * [x] Wgpu + Winit
  * [x] Simple frame management and buffer upload system
  * [x] Robust key binding support
  * [x] Basic command system
  * [ ] VR support
  * [ ] Joystick support
  * [ ] Gamepad support
* Atmospheric Simulation
  * [x] Basic precomputed scattering: Using [Bruneton's method](https://github.com/ebruneton/precomputed_atmospheric_scattering).
  * [ ] Dynamically changing atmospheric conditions
  * [ ] Spatially variable atmospheric parameters
* Weather Simulation
  * [ ] Pick a technique to apply for both global and local scales
* Water Simulation
  * [ ] Pick a research paper to implement
* Cloud Simulation
  * [x] Pick a technique to implement: Using [Horizon Zero Dawn's method](http://advances.realtimerendering.com/s2015/The%20Real-time%20Volumetric%20Cloudscapes%20of%20Horizon%20-%20Zero%20Dawn%20-%20ARTR.pdf)
  * [ ] Noise based cloud layer generation and management
  * [ ] Cloud light scattering implementation
* Forest Simulation
  * [ ] Pick a technique to implement
      * Candidate: [Bruneton's Real Time Realistic Rendering and Lighting of Forests](https://hal.inria.fr/hal-00650120/file/article.pdf)
* Script Driven Game Engine
  * [x] Pick a good name: Nitrous
  * [x] Basic scripting engine
  * [x] drop-down console
  * [x] command history
  * [x] ECS Driven Memory System
  * [ ] pretty output and entities lists
  * [ ] Scripted Functions
* Flight Modeling
  * [x] Pick an algorithm: [Allerton's Principles of Flight Simulation](https://www.wiley.com/en-us/Principles+of+Flight+Simulation-p-9780470754368)
  * [ ] Expose relevant controls and surfaces
  * [ ] Framework for providing forces and moments to the flight model
* Entity/Runtime System
  * [ ] Save/Load support
  * [ ] Replay recording
  * [ ] Network syncing
* Planetary Scale Rendering; Using [Kooima's thesis](https://www.evl.uic.edu/documents/kooima-dissertation-uic.pdf).
  * [x] Patch management
  * [x] Patch tesselation
  * [x] Heightmap generator
  * [x] Colormap generator
  * [ ] L14+ data
  * [ ] polar projection data
  * [ ] web hosted data
  * [x] Atmospheric blending
  * [ ] Self shadowing
* Text
  * [x] Layout management
  * [x] TTF loading
  * [x] 2d screen-space rendering
  * [ ] in-world text rendering
* UI
  * [x] Basic UI Framework
  * [x] Blurred backgrounds
  * [x] Labels
  * [x] Line editing
  * [x] Vertical Box
  * [x] Expander
  * [x] Console
  * [ ] In-world UI elements
* Sound
  * [ ] Pick a framework
  * [ ] Sample management
  * [ ] Channel management and blending
  * [ ] Positional audio
  * [ ] Frequency scaling (e.g. for wind and engine noises)
  * [ ] Doppler effects
