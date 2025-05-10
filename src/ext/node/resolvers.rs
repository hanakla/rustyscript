use deno_ast::{MediaType, ModuleSpecifier};
use deno_node::{NodeExtInitServices, NodeRequireLoader, NodeResolver, NodeResolverRc};
use deno_resolver::{
    npm::{ByonmNpmResolver, ByonmNpmResolverCreateOptions},
};
use node_resolver::{errors::{ClosestPkgJsonError, }, DenoIsBuiltInNodeModuleChecker, InNpmPackageChecker, NpmPackageFolderResolver, PackageJsonResolver, PackageJsonResolverRc, ResolutionMode, UrlOrPathRef};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc},
};
use std::ops::Deref;
use deno_error::JsErrorBox;
use deno_fs::sync::MaybeArc;
use deno_resolver::npm::{CreateInNpmPkgCheckerOptions, DenoInNpmPackageChecker, NpmResolver};
use deno_semver::package::PackageReq;
use node_resolver::analyze::{CjsAnalysis, CjsAnalysisExports, CjsCodeAnalyzer};
use node_resolver::errors::{PackageFolderResolveErrorKind, PackageNotFoundError};
use reqwest::Url;
use sys_traits::FsRead;
use sys_traits::impls::RealSys;

const NODE_MODULES_DIR: &str = "node_modules";

/// Package resolver for the `deno_node` extension
#[derive(Debug)]
pub struct RustyResolver {
    pub fs: RealSys,
    pub byonm: NpmResolver<RealSys>,
    pub pjson_resolver: PackageJsonResolverRc<RealSys>,
    pub node_require_loader: Rc<RustyRequireLoader>,
    pub node_resolver: NodeResolverRc<DenoInNpmPackageChecker, NpmResolver<RealSys>, RealSys>,
}

impl Default for RustyResolver {
    fn default() -> Self {
        Self::new(None, RealSys::default())
    }
}

impl RustyResolver {
    /// Create a new resolver with the given base directory and filesystem
    pub fn new(base_dir: Option<PathBuf>, fs: RealSys) -> Self {
        let mut base = base_dir;
        if base.is_none() {
            base = std::env::current_dir().ok();
        }

        let arcFs = Arc::new(fs.clone());

        let root_node_modules_dir = base.map(|mut p| {
            p.push(NODE_MODULES_DIR);
            p
        });

        let pjson_resolver =
            MaybeArc::new(PackageJsonResolver::new(fs.clone(), None));

        let node_require_loader = Rc::new(RustyRequireLoader(arcFs.clone()));

        let byonm = NpmResolver::Byonm(
            MaybeArc::new(ByonmNpmResolver::new(ByonmNpmResolverCreateOptions {
                root_node_modules_dir: root_node_modules_dir.clone(),
                pkg_json_resolver: pjson_resolver.clone(),
                sys: fs.clone(),
            }))
        );

        let node_resolver = MaybeArc::new(NodeResolver::new(
            DenoInNpmPackageChecker::new(CreateInNpmPkgCheckerOptions::Byonm),
            DenoIsBuiltInNodeModuleChecker {},
            byonm.clone(),
            pjson_resolver.clone(),
            fs.clone(),
            node_resolver::ConditionsFromResolutionMode::default()
        ));

        Self {
            pjson_resolver,
            node_require_loader,
            fs,
            node_resolver,
            byonm,

            // known: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_node_init_services(&self) ->
        NodeExtInitServices<
            DenoInNpmPackageChecker,
            NpmResolver<RealSys>,
            RealSys
        >
    {
        NodeExtInitServices {
            node_require_loader: self.node_require_loader.clone(),
            node_resolver: self.node_resolver.clone(),
            pkg_json_resolver: self.pjson_resolver.clone(),
            sys: self.fs.clone(),
        }
    }

    pub fn node_reslover(self: &Arc<Self>) -> NodeResolver<DenoInNpmPackageChecker, NpmResolver<RealSys>, RealSys> {
        NodeResolver::new(
            DenoInNpmPackageChecker::new(CreateInNpmPkgCheckerOptions::Byonm),
            DenoIsBuiltInNodeModuleChecker {},
            self.byonm.clone(),
            self.pjson_resolver.clone(),
            self.fs.clone(),
            node_resolver::ConditionsFromResolutionMode::default()
        )
    }
}

impl InNpmPackageChecker for RustyResolver {
    fn in_npm_package(&self, specifier: &reqwest::Url) -> bool {
        let is_file = specifier.scheme() == "file";

        let path = specifier.path().to_ascii_lowercase();
        let in_node_modules = path.contains("/node_modules/");
        let is_polyfill = path.contains("/node:");

        is_file && (in_node_modules || is_polyfill)
    }
}

impl NpmPackageFolderResolver for RustyResolver {
    fn resolve_package_folder_from_package(
        &self,
        specifier: &str,
        referrer: &UrlOrPathRef
    ) -> Result<PathBuf, node_resolver::errors::PackageFolderResolveError> {
        let request = PackageReq::from_str(specifier).map_err(|_| {
            let e = Box::new(PackageFolderResolveErrorKind::PackageNotFound(
                PackageNotFoundError {
                    package_name: specifier.to_string(),
                    referrer: referrer.display(),
                    referrer_extra: None,
                },
            ));
            node_resolver::errors::PackageFolderResolveError(e)
        })?;

        let p = self
            .byonm
            .resolve_pkg_folder_from_deno_module_req(&request, deno_path_util::url_from_file_path(&referrer.display()));
        match p {
            Ok(p) => Ok(p),
            Err(_) => self
                .byonm
                .resolve_package_folder_from_package(specifier, referrer),
        }
    }
}

#[derive(Debug)]
struct RustyRequireLoader(Arc<RealSys>);
impl NodeRequireLoader for RustyRequireLoader {
    fn load_text_file_lossy(
        &self,
        path: &Path,
    ) -> Result<Cow<'static, str>, JsErrorBox> {
        let media_type = MediaType::from_path(path);
        self.0.fs_read_to_string_lossy(path)
            .map_err(JsErrorBox::from_err)
    }

    fn ensure_read_permission<'a>(
        &self,
        permissions: &mut dyn deno_node::NodePermissions,
        path: &'a Path,
    ) -> Result<std::borrow::Cow<'a, Path>, JsErrorBox> {
        let is_in_node_modules = path
            .components()
            .all(|c| c.as_os_str().to_ascii_lowercase() != NODE_MODULES_DIR);
        if is_in_node_modules {
            match permissions.check_read_path(path) {
                Ok(result) => Ok(result),
                Err(e) => Err(JsErrorBox::new("SecurityError", e.to_string())),
            }
        } else {
            Ok(Cow::Borrowed(path))
        }
    }

    fn is_maybe_cjs(&self, specifier: &reqwest::Url) -> Result<bool, ClosestPkgJsonError> {
        if specifier.scheme() != "file" {
            return Ok(false);
        }

        match MediaType::from_specifier(specifier) {
            MediaType::Wasm
            | MediaType::Json
            | MediaType::Mts
            | MediaType::Mjs
            | MediaType::Dmts => Ok(false),

            _ => Ok(true),
        }
    }
}
impl Clone for RustyRequireLoader {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}


// SEE: https://github.com/denoland/deno/blob/56f67b58511d59c5da4b62aec1dced30a17b5de4/cli/node.rs#L154
pub struct RustyCjsCodeAnalyzer {
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CliCjsAnalysis {
    /// The module was found to be an ES module.
    Esm,
    /// The module was CJS.
    Cjs {
        exports: Vec<String>,
        reexports: Vec<String>,
    },
}

impl RustyCjsCodeAnalyzer {
    async fn inner_cjs_analysis(
        &self,
        specifier: &ModuleSpecifier,
        source: &str,
    ) -> Result<CliCjsAnalysis, JsErrorBox> {
        let source_hash = CacheDBHash::from_hashable(source);
        if let Some(analysis) =
            self.cache.get_cjs_analysis(specifier.as_str(), source_hash)
        {
            return Ok(analysis);
        }

        let media_type = MediaType::from_specifier(specifier);
        if media_type == MediaType::Json {
            return Ok(CliCjsAnalysis::Cjs {
                exports: vec![],
                reexports: vec![],
            });
        }

        let cjs_tracker = self.cjs_tracker.clone();
        let is_maybe_cjs = cjs_tracker
            .is_maybe_cjs(specifier, media_type)
            .map_err(JsErrorBox::from_err)?;
        let analysis = if is_maybe_cjs {
            let maybe_parsed_source = self
                .parsed_source_cache
                .as_ref()
                .and_then(|c| c.remove_parsed_source(specifier));

            deno_core::unsync::spawn_blocking({
                let specifier = specifier.clone();
                let source: Arc<str> = source.into();
                move || -> Result<_, JsErrorBox> {
                    let parsed_source = maybe_parsed_source
                        .map(Ok)
                        .unwrap_or_else(|| {
                            deno_ast::parse_program(deno_ast::ParseParams {
                                specifier,
                                text: source,
                                media_type,
                                capture_tokens: true,
                                scope_analysis: false,
                                maybe_syntax: None,
                            })
                        })
                        .map_err(JsErrorBox::from_err)?;
                    let is_script = parsed_source.compute_is_script();
                    let is_cjs = cjs_tracker
                        .is_cjs_with_known_is_script(
                            parsed_source.specifier(),
                            media_type,
                            is_script,
                        )
                        .map_err(JsErrorBox::from_err)?;
                    if is_cjs {
                        let analysis = parsed_source.analyze_cjs();
                        Ok(CliCjsAnalysis::Cjs {
                            exports: analysis.exports,
                            reexports: analysis.reexports,
                        })
                    } else {
                        Ok(CliCjsAnalysis::Esm)
                    }
                }
            })
                .await
                .unwrap()?
        } else {
            CliCjsAnalysis::Esm
        };

        self
            .cache
            .set_cjs_analysis(specifier.as_str(), source_hash, &analysis);

        Ok(analysis)
    }
}

impl CjsCodeAnalyzer for RustyCjsCodeAnalyzer {

    async fn analyze_cjs<'a>(&self, specifier: &Url, maybe_source: Option<Cow<'a, str>>) -> Result<CjsAnalysis<'a>, JsErrorBox> {
        let source = match source {
            Some(source) => source,
            None => {
                if let Ok(path) = specifier.to_file_path() {
                    if let Ok(source_from_file) =
                        self.fs.read_text_file_lossy_async(path, None).await
                    {
                        source_from_file
                    } else {
                        return Ok(CjsAnalysis::Cjs(CjsAnalysisExports {
                            exports: vec![],
                            reexports: vec![],
                        }));
                    }
                } else {
                    return Ok(CjsAnalysis::Cjs(CjsAnalysisExports {
                        exports: vec![],
                        reexports: vec![],
                    }));
                }
            }
        };
        let analysis = self.inner_cjs_analysis(specifier, &source).await?;
        match analysis {
            CliCjsAnalysis::Esm => Ok(CjsAnalysis::Esm(source)),
            CliCjsAnalysis::Cjs { exports, reexports } => {
                Ok(CjsAnalysis::Cjs(CjsAnalysisExports {
                    exports,
                    reexports,
                }))
            }
        }
    }
}
