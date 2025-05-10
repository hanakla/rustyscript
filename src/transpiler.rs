// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.
//! This file transpiles TypeScript and JSX/TSX
//! modules.
//!
//! It will only transpile, not typecheck (like Deno's `--no-check` flag).

use deno_ast::MediaType;
use deno_core::FastString;
use deno_core::ModuleSpecifier;
use deno_core::SourceMapData;
use std::borrow::Cow;
use std::rc::Rc;
use deno_error::JsErrorBox;

pub type ModuleContents = (FastString, Option<SourceMapData>);

fn should_transpile(media_type: MediaType) -> bool {
    matches!(
        media_type,
        MediaType::Jsx
            | MediaType::TypeScript
            | MediaType::Mts
            | MediaType::Cts
            | MediaType::Dts
            | MediaType::Dmts
            | MediaType::Dcts
            | MediaType::Tsx
    )
}

///
/// Transpiles source code from TS to JS without typechecking
pub fn transpile(module_specifier: &ModuleSpecifier, code: &str) -> Result<ModuleContents, JsErrorBox> {
    deno_runtime::transpile::maybe_transpile_source(
        FastString::from(module_specifier.clone()),
        FastString::from(code.to_string()),
    )
}

///
/// Transpile an extension
#[allow(clippy::type_complexity)]
pub fn transpile_extension(
    specifier: &ModuleSpecifier,
    code: &str,
) -> Result<(FastString, Option<Cow<'static, [u8]>>), JsErrorBox> {
    let (code, source_map) = transpile(&specifier, code)?;
    let code = FastString::from(code);
    Ok((code, source_map))
}

pub type ExtensionTranspiler =
    Rc<dyn Fn(FastString, FastString) -> Result<(FastString, Option<Cow<'static, [u8]>>), JsErrorBox>>;
pub type ExtensionTranspilation = (FastString, Option<Cow<'static, [u8]>>);
