use std::sync::Arc;
use deno_core::Extension;
use deno_os::ExitCode;
use crate::ext::ExtensionTrait;

impl ExtensionTrait<OsOptions> for deno_os::deno_os {
    fn init(options: OsOptions) -> Extension {
        deno_os::deno_os::init_ops_and_esm(options.exit_code)
    }
}

pub fn extensions(
    options: OsOptions,
    is_snapshot: bool,
) -> Vec<Extension> {
    vec![
        deno_os::deno_os::build(options, is_snapshot),
    ]
}

#[derive(Clone)]
pub struct OsOptions {
    pub exit_code: ExitCode
}

impl Default for OsOptions {
    fn default() -> Self {
        Self {
            exit_code: ExitCode::default()
        }
    }
}