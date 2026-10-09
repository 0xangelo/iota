//! Package compilation, mirroring `iota move test` (test plan) and `iota move
//! coverage --dev` (summary modules) without their exit-on-error diagnostics
//! reporting.

use std::path::Path;

use anyhow::anyhow;
use move_binary_format::{CompiledModule, binary_config::BinaryConfig};
use move_compiler::{
    PASS_CFGIR,
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::{Diagnostics, report_diagnostics_to_buffer},
    shared::files::MappedFiles,
    unit_test::{TestPlan, plan_builder::construct_test_plan},
};
use move_package::{BuildConfig, compilation::build_plan::BuildPlan};

use crate::{Error, Result};

/// Compile in dev and test mode, and plan every `#[test]` of the root package.
pub(crate) fn test_plan(package: &Path, build_dir: &Path) -> Result<TestPlan> {
    let config = BuildConfig {
        dev_mode: true,
        test_mode: true,
        ..build_config(build_dir)
    };
    let graph = config
        .resolution_graph_for_package(package, None, &mut std::io::sink())
        .map_err(build_error)?;

    // Packages without sources are bytecode dependencies: the compiler doesn't
    // return them, but the tests' storage needs them.
    let binary_config = BinaryConfig::new_unpublishable();
    let mut bytecode_deps = Vec::new();
    for dependency in graph.package_table.values() {
        if !dependency
            .get_sources(&graph.build_options)
            .map_err(build_error)?
            .is_empty()
        {
            continue;
        }
        for bytes in dependency.get_bytecodes_bytes().map_err(build_error)? {
            bytecode_deps.push(
                CompiledModule::deserialize_with_config(&bytes, &binary_config)
                    .map_err(|e| Error::Build(e.into()))?,
            );
        }
    }

    let root = graph.root_package();
    let mut planned = None;
    BuildPlan::create(&graph)
        .map_err(build_error)?
        .compile_with_driver(&mut std::io::sink(), |compiler| {
            let (files, stepped) = compiler.run::<PASS_CFGIR>()?;
            let stepped = stepped.map_err(|(_, diags)| diagnostics_error(&files, diags))?;
            let (compiler, cfgir) = stepped.into_ast();
            let env = compiler.compilation_env();
            let tests = construct_test_plan(env, Some(root), &cfgir);
            let mapped_files = env.mapped_files().clone();
            let (units, _warnings) = compiler
                .at_cfgir(cfgir)
                .build()
                .map_err(|(_, diags)| diagnostics_error(&files, diags))?;
            let modules = units.iter().map(|unit| unit.named_module.clone()).collect();
            planned = Some((tests, mapped_files, modules));
            Ok((files, units))
        })
        .map_err(build_error)?;

    let (tests, mapped_files, modules) = planned.expect("the driver ran to completion");
    Ok(TestPlan::new(
        tests.unwrap_or_default(),
        mapped_files,
        modules,
        bytecode_deps,
    ))
}

/// Compile in dev mode, not test mode, and return the root package's modules in
/// the order the compiler emitted them: the module set and order `iota move
/// coverage --dev summary` prints.
pub(crate) fn root_modules(package: &Path, build_dir: &Path) -> Result<Vec<CompiledModule>> {
    let config = BuildConfig {
        dev_mode: true,
        ..build_config(build_dir)
    };
    let graph = config
        .resolution_graph_for_package(package, None, &mut std::io::sink())
        .map_err(build_error)?;
    let compiled = BuildPlan::create(&graph)
        .map_err(build_error)?
        .compile_with_driver(&mut std::io::sink(), build_units)
        .map_err(build_error)?;
    Ok(compiled
        .root_modules()
        .map(|unit| unit.unit.module.clone())
        .collect())
}

/// The CLI's build configuration: IOTA flavor and implicit system dependencies.
fn build_config(build_dir: &Path) -> BuildConfig {
    let mut config = BuildConfig {
        install_dir: Some(build_dir.to_owned()),
        skip_fetch_latest_git_deps: true,
        ..Default::default()
    };
    if let Some(err) = iota_move_build::set_iota_flavor(&mut config) {
        unreachable!("the default flavor is unset: {err}");
    }
    config.implicit_dependencies = iota_move_build::implicit_deps(
        iota_package_management::system_package_versions::latest_system_packages(),
    );
    config
}

fn build_units(
    compiler: move_compiler::Compiler,
) -> anyhow::Result<(MappedFiles, Vec<AnnotatedCompiledUnit>)> {
    let (files, units) = compiler.build()?;
    let (units, _warnings) = units.map_err(|diags| diagnostics_error(&files, diags))?;
    Ok((files, units))
}

fn diagnostics_error(files: &MappedFiles, diags: Diagnostics) -> anyhow::Error {
    let rendered = report_diagnostics_to_buffer(files, diags, false);
    anyhow!("{}", String::from_utf8_lossy(&rendered))
}

fn build_error(err: anyhow::Error) -> Error {
    Error::Build(format!("{err:#}").into())
}
