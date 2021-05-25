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

/**
 * Scripts should be put in files like: <project>/shaders/<name>.<type>.glsl
 * Outputs get put in the project target dir: <project>/target/<name>.<type>.spir
 *
 * Compiler Options:
 *     DUMP_SPIRV=1   Dump disassembled code next to bytecode.
 *     DEBUG=1        Compile with debug settings.
 */
use anyhow::{bail, Result};
use log::trace;
use shaderc::{
    CompilationArtifact, CompileOptions, Compiler, Error, IncludeType, OptimizationLevel,
    ResolvedInclude, ShaderKind,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn type_for_filename(name: &str) -> ShaderKind {
    if name.ends_with(".vert.glsl") {
        ShaderKind::Vertex
    } else if name.ends_with(".frag.glsl") {
        ShaderKind::Fragment
    } else if name.ends_with(".comp.glsl") {
        ShaderKind::Compute
    } else if name.ends_with(".tess.ctrl.glsl") {
        ShaderKind::TessControl
    } else if name.ends_with(".tess.eval.glsl") {
        ShaderKind::TessEvaluation
    } else {
        ShaderKind::InferFromSource
    }
}

fn output_for_name(name: &str) -> String {
    assert!(name.ends_with(".glsl"));
    assert!(name.len() > 5);
    let file_name = format!("{}.spirv", &name[..name.len() - 5]);

    let project_cargo_root = env::var("CARGO_MANIFEST_DIR").unwrap();

    let target_dir = Path::new(&project_cargo_root).join("target");
    trace!("creating directory: {:?}", target_dir);
    fs::create_dir_all(&target_dir).expect("a directory");

    let target = target_dir.join(file_name);
    trace!("generating: {:?}", target);
    target.to_str().expect("a file").to_owned()
}

fn decorate_error(msg: &str) -> String {
    msg.replace(" error: ", " \x1B[91merror\x1B[0m: ")
}

fn find_included_file(
    name: &str,
    _include_type: IncludeType,
    _source_file: &str,
    _include_depth: usize,
) -> Result<ResolvedInclude, String> {
    // This is not the manifest dir, it is the project dir. Walk up to find the libs directory.
    let project_cargo_root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut libs_dir = Path::new(&project_cargo_root);
    while libs_dir.file_stem().expect("non-root") != "libs" {
        libs_dir = libs_dir.parent().expect("non-root");
    }
    assert_eq!(libs_dir.file_stem().expect("non-root"), "libs");

    // The directories that will act as the base for #include<> directives.
    let mut include_dirs = vec![libs_dir.to_owned(), libs_dir.join("render-wgpu")];

    // The manifest dir may or may not be in the nitrogen project; nitrogen might instead be
    // linked in under the libs dir. If so, add nitrogen to the includes, so that nitrogen
    // includes are available to the client.
    if libs_dir.join("nitrogen").is_dir() {
        include_dirs.push(libs_dir.join("nitrogen"));
    }

    let input_path: PathBuf = name.split('/').collect();
    trace!("Using include dirs: {:?}", include_dirs);
    for path in &include_dirs {
        let attempt = path.join(&input_path);
        trace!("Checking: {:?}", attempt);
        if attempt.is_file() {
            let resolved_name = attempt.to_str().expect("a path").to_owned();
            println!("cargo:rerun-if-changed={}", resolved_name);
            return Ok(ResolvedInclude {
                resolved_name,
                content: fs::read_to_string(attempt).expect("file content"),
            });
        }
    }
    Err("NOT_FOUND".to_owned())
}

pub fn build() -> Result<()> {
    println!("cargo:rerun-if-env-changed=DUMP_SPIRV");
    println!("cargo:rerun-if-env-changed=DEBUG");
    let shaders_dir = env::current_dir()?.as_path().join("shaders");
    if shaders_dir.is_dir() {
        println!(
            "cargo:rerun-if-changed={}/",
            shaders_dir.to_str().expect("a path")
        );
    }
    let include_dir = env::current_dir()?.as_path().join("include");
    if include_dir.is_dir() {
        println!(
            "cargo:rerun-if-changed={}/",
            include_dir.to_str().expect("a path")
        );
    }

    let shader_dir = Path::new("shaders/");
    if !shader_dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(shader_dir)? {
        let entry = entry?;
        let pathbuf = entry.path();
        if !pathbuf.is_file() {
            continue;
        }
        let dump_spirv = env::var("DUMP_SPIRV").unwrap_or_else(|_| "0".to_owned()) == "1";
        let (spirv, assembly) = build_shader_from_path(&pathbuf, dump_spirv)?;
        let target_path = output_for_name(
            pathbuf
                .file_name()
                .expect("a file name")
                .to_str()
                .expect("a string"),
        );
        fs::write(&target_path, spirv.as_binary_u8())?;

        if let Some(spirv_assembly) = assembly {
            fs::write(&format!("{}.s", target_path), spirv_assembly.as_text())?;
        }

        {
            let options = naga::front::spv::Options {
                adjust_coordinate_space: false, // we require NDC_Y_UP feature
                strict_capabilities: true,
                flow_graph_dump_prefix: None,
            };
            let parser = naga::front::spv::Parser::new(spirv.as_binary().iter().cloned(), &options);
            match parser.parse() {
                Ok(_module) => {}
                Err(err) => {
                    let msg = format!(
                        "Failed to parse shader SPIR-V code for {:?}: {:?}",
                        pathbuf, err
                    );
                    log::warn!("{}", &msg);
                    //bail!(msg)
                }
            };
        }
    }

    Ok(())
}

pub fn build_shader_from_path(
    path: &Path,
    dump_spirv: bool,
) -> Result<(CompilationArtifact, Option<CompilationArtifact>)> {
    let path_str = path.to_str().expect("a filename");

    let shader_content = fs::read_to_string(path)?;
    let shader_type = type_for_filename(&path_str);

    let mut options = CompileOptions::new().expect("some options");
    options.set_warnings_as_errors();
    let opt_level = if env::var("DEBUG").unwrap_or_else(|_| "0".to_owned()) == "1" {
        options.set_generate_debug_info();
        OptimizationLevel::Zero
    } else {
        OptimizationLevel::Performance
    };
    options.set_optimization_level(opt_level);
    options.set_include_callback(find_included_file);

    let mut compiler = Compiler::new().expect("a compiler");
    let result = compiler.compile_into_spirv(
        &shader_content,
        shader_type,
        path_str,
        "main",
        Some(&options),
    );
    if let Err(Error::CompilationError(_, ref msg)) = result {
        println!("{}", decorate_error(msg));
        bail!("failed to compile");
    }
    let spirv_assembly = if dump_spirv {
        Some(compiler.compile_into_spirv_assembly(
            &shader_content,
            shader_type,
            path_str,
            "main",
            Some(&options),
        )?)
    } else {
        None
    };
    Ok((result?, spirv_assembly))
}
