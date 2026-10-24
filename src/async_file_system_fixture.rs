// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed asynchronous fixtures for filesystem contract suites.

use std::future::Future;
use std::pin::Pin;

use qubit_fs as qfs;
use qubit_fs::AsyncFileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::path::Path;

use crate::AsyncCopyCancellationStage;
use crate::AsyncWriteCancellationStage;
use crate::CopyCancellationProbe;
use crate::CopyFixtureCase;
use crate::FixtureResult;
use crate::FixtureSupport;
use crate::WriteCancellationProbe;

/// Runtime-neutral future returned by asynchronous fixture observations.
///
/// # Type Parameters
///
/// * `'a` - Lifetime shared by the fixture and borrowed request data.
/// * `T` - Successful value produced by the asynchronous hook.
///
/// # Examples
///
/// ```
/// use qubit_fs_testkit::FixtureFuture;
///
/// let future: FixtureFuture<'_, i32> = Box::pin(async { Ok(42) });
/// let _ = future;
/// ```
pub type FixtureFuture<'a, T> = Pin<Box<dyn Future<Output = FixtureResult<T>> + Send + 'a>>;

/// Supplies an isolated asynchronous facade and optional provider observations.
///
/// # Examples
///
/// ```
/// # mod support { include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/rustdoc_support.rs")); }
/// # use support::RustdocAsyncFixture;
/// use qubit_fs_testkit::{AsyncFileSystemContractSuite, AsyncFileSystemFixture};
///
/// let fixture = RustdocAsyncFixture::new();
/// let suite = AsyncFileSystemContractSuite::new(&fixture);
/// assert!(suite.run().report().checks().is_empty());
/// ```
pub trait AsyncFileSystemFixture: Sync {
    /// Snapshots every logical path in the isolated configured namespace.
    ///
    /// Use the underlying SDK or fixture model, never the facade list method.
    /// Include resources created by earlier checks, not only the current seed.
    /// Returns unsupported when independent completeness cannot be established.
    ///
    /// # Returns
    ///
    /// A future resolving to every path in the isolated namespace, or
    /// `Unsupported` when the fixture cannot establish a complete snapshot.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when the
    /// independent snapshot operation fails.
    fn snapshot_namespace_paths(&self) -> FixtureFuture<'_, FixtureSupport<Vec<Path>>> {
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Independently prepares a copy source and a scenario-specific target.
    ///
    /// File scenarios contain the supplied bytes and use file options, with
    /// Required atomicity or durability for the respective strong scenario.
    /// ServerSide cases use provider-selected observable bytes and require
    /// server-side copy. Ready cases have distinct paths,
    /// and Conflict targets contain `b"existing"`.
    /// Teardown must clean partial setup even when preparation fails.
    ///
    /// # Parameters
    ///
    /// * `scenario` - Copy behavior and guarantees to prepare.
    /// * `source_relative` - Testkit-relative source name.
    /// * `target_relative` - Testkit-relative destination name.
    /// * `bytes` - Source content for file-copy scenarios and the child file
    ///   for tree-copy scenarios.
    ///
    /// # Returns
    ///
    /// A future resolving to `Ready(case)` when setup succeeds, or
    /// `Unavailable` when required fixture setup is unsupported.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when path
    /// mapping or an out-of-band preparation operation fails.
    fn prepare_copy<'a>(
        &'a self,
        scenario: crate::CopyScenario,
        source_relative: &'a str,
        target_relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, crate::FixturePreparation<crate::CopyFixtureCase>> {
        Box::pin(async move {
            if scenario == crate::CopyScenario::ServerSide {
                return Ok(match self.copy_fast_path_case(CopyMethod::ServerSide).await? {
                    FixtureSupport::Supported(case) => crate::FixturePreparation::Ready(case),
                    FixtureSupport::Unsupported => crate::FixturePreparation::Unavailable {
                        reason: "fixture has no server-side copy case".to_owned(),
                    },
                });
            }
            if matches!(
                scenario,
                crate::CopyScenario::AtomicTree | crate::CopyScenario::DurableTree
            ) {
                let FixtureSupport::Supported(source) = self.seed_empty_directory(source_relative).await? else {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "tree root setup unavailable".to_owned(),
                    });
                };
                let sub_relative = format!("{source_relative}/sub");
                if matches!(
                    self.seed_empty_directory(&sub_relative).await?,
                    FixtureSupport::Unsupported
                ) {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "tree subdirectory setup unavailable".to_owned(),
                    });
                }
                let child_relative = format!("{source_relative}/sub/child");
                if matches!(
                    self.seed_file(&child_relative, bytes).await?,
                    FixtureSupport::Unsupported
                ) {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "tree child setup unavailable".to_owned(),
                    });
                }
                let target = self.path(target_relative)?;
                let options = if scenario == crate::CopyScenario::AtomicTree {
                    qfs::copy::CopyOptions::tree().with_atomicity(qfs::metadata::AtomicityRequirement::Required)
                } else {
                    qfs::copy::CopyOptions::tree().with_durability(qfs::metadata::DurabilityRequirement::Required)
                };
                return Ok(crate::FixturePreparation::Ready(crate::CopyFixtureCase::new(
                    source, target, options,
                )));
            }
            let FixtureSupport::Supported(source) = self.seed_file(source_relative, bytes).await? else {
                return Ok(crate::FixturePreparation::Unavailable {
                    reason: "copy source seed unavailable".to_owned(),
                });
            };
            let target = if scenario == crate::CopyScenario::Conflict {
                let FixtureSupport::Supported(target) = self.seed_file(target_relative, b"existing").await? else {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "copy conflict target seed unavailable".to_owned(),
                    });
                };
                target
            } else {
                self.path(target_relative)?
            };
            let options = match scenario {
                crate::CopyScenario::AtomicFile => {
                    qfs::copy::CopyOptions::file().with_atomicity(qfs::metadata::AtomicityRequirement::Required)
                }
                crate::CopyScenario::DurableFile => {
                    qfs::copy::CopyOptions::file().with_durability(qfs::metadata::DurabilityRequirement::Required)
                }
                _ => qfs::copy::CopyOptions::file(),
            };
            Ok(crate::FixturePreparation::Ready(crate::CopyFixtureCase::new(
                source, target, options,
            )))
        })
    }

    /// Independently prepares an existing file for a selected deletion
    /// scenario. Version hooks must provide evidence for
    /// `DeleteScenario::IfMatch`.
    ///
    /// # Parameters
    ///
    /// * `scenario` - Deletion behavior to prepare.
    /// * `relative` - Testkit-relative name of the file to seed.
    /// * `bytes` - Exact initial file content.
    ///
    /// # Returns
    ///
    /// A future resolving to the prepared path, or `Unavailable` when the
    /// fixture cannot seed the file.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when seeding
    /// fails.
    fn prepare_delete<'a>(
        &'a self,
        _scenario: crate::DeleteScenario,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, crate::FixturePreparation<Path>> {
        Box::pin(async move {
            Ok(match self.seed_file(relative, bytes).await? {
                FixtureSupport::Supported(path) => crate::FixturePreparation::Ready(path),
                FixtureSupport::Unsupported => crate::FixturePreparation::Unavailable {
                    reason: "fixture cannot seed deletion scenario".to_owned(),
                },
            })
        })
    }

    /// Independently prepares one read scenario and its initial content.
    ///
    /// A Ready path must contain the supplied bytes, except for the explicit
    /// corruption scenario. Version observations remain independent fixture
    /// hooks. Preparation errors must not become inapplicability declarations.
    ///
    /// # Parameters
    ///
    /// * `scenario` - Read behavior to prepare.
    /// * `relative` - Testkit-relative name of the resource.
    /// * `bytes` - Exact initial content, except for checksum corruption.
    ///
    /// # Returns
    ///
    /// A future resolving to the prepared path, or `Unavailable` when the
    /// fixture cannot independently prepare the selected scenario.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when setup or
    /// checksum-case preparation fails.
    fn prepare_read<'a>(
        &'a self,
        scenario: crate::ReadScenario,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, crate::FixturePreparation<Path>> {
        Box::pin(async move {
            let prepared = if scenario == crate::ReadScenario::ChecksumCorruption {
                self.checksum_failure_case(relative).await?
            } else {
                self.seed_file(relative, bytes).await?
            };
            Ok(match prepared {
                FixtureSupport::Supported(path) => crate::FixturePreparation::Ready(path),
                FixtureSupport::Unsupported => crate::FixturePreparation::Unavailable {
                    reason: "fixture cannot independently prepare this read scenario".to_owned(),
                },
            })
        })
    }

    /// Independently prepares the selected write request and initial state.
    ///
    /// The default prepares CreateNew and durable creation requests, and seeds
    /// targets for atomic replacement, creation conflicts and append
    /// independently. Missing setup remains explicit and is never counted
    /// as successful testing.
    ///
    /// # Parameters
    ///
    /// * `scenario` - Write behavior and guarantees to prepare.
    /// * `relative` - Testkit-relative target name.
    /// * `bytes` - Payload submitted by the write operation.
    ///
    /// # Returns
    ///
    /// A future resolving to the prepared write request, or `Unavailable`
    /// when required setup is unsupported.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when path
    /// mapping or out-of-band seeding fails.
    fn prepare_write<'a>(
        &'a self,
        scenario: crate::WriteScenario,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, crate::FixturePreparation<crate::WriteFixtureCase>> {
        Box::pin(async move {
            if scenario == crate::WriteScenario::Replace {
                let FixtureSupport::Supported(path) = self.seed_file(relative, b"previous contents").await? else {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "replacement seed unavailable".to_owned(),
                    });
                };
                return Ok(crate::FixturePreparation::Ready(crate::WriteFixtureCase::new(
                    path,
                    bytes.to_vec(),
                    qfs::write::WriteOptions::default(),
                )));
            }
            if scenario == crate::WriteScenario::CreateConflict {
                let FixtureSupport::Supported(path) = self.seed_file(relative, b"a").await? else {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "creation conflict seed unavailable".to_owned(),
                    });
                };
                return Ok(crate::FixturePreparation::Ready(crate::WriteFixtureCase::new(
                    path,
                    bytes.to_vec(),
                    qfs::write::WriteOptions::default().with_disposition(qfs::write::WriteDisposition::CreateNew),
                )));
            }
            if scenario == crate::WriteScenario::Append {
                let FixtureSupport::Supported(path) = self.seed_file(relative, b"before").await? else {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "append seed unavailable".to_owned(),
                    });
                };
                return Ok(crate::FixturePreparation::Ready(crate::WriteFixtureCase::new(
                    path,
                    bytes.to_vec(),
                    qfs::write::WriteOptions::default().with_disposition(qfs::write::WriteDisposition::Append),
                )));
            }
            if scenario == crate::WriteScenario::AtomicReplace {
                let FixtureSupport::Supported(path) = self.seed_file(relative, b"a").await? else {
                    return Ok(crate::FixturePreparation::Unavailable {
                        reason: "atomic replacement seed unavailable".to_owned(),
                    });
                };
                return Ok(crate::FixturePreparation::Ready(crate::WriteFixtureCase::new(
                    path,
                    bytes.to_vec(),
                    qfs::write::WriteOptions::default().with_atomicity(qfs::metadata::AtomicityRequirement::Required),
                )));
            }
            if scenario == crate::WriteScenario::Durable {
                return Ok(crate::FixturePreparation::Ready(crate::WriteFixtureCase::new(
                    self.path(relative)?,
                    bytes.to_vec(),
                    qfs::write::WriteOptions::default()
                        .with_disposition(qfs::write::WriteDisposition::CreateNew)
                        .with_durability(qfs::metadata::DurabilityRequirement::Required),
                )));
            }
            if !matches!(scenario, crate::WriteScenario::Create | crate::WriteScenario::Abort) {
                return Ok(crate::FixturePreparation::Unavailable {
                    reason: "fixture does not prepare this write scenario".to_owned(),
                });
            }
            Ok(crate::FixturePreparation::Ready(crate::WriteFixtureCase::new(
                self.path(relative)?,
                bytes.to_vec(),
                qfs::write::WriteOptions::default().with_disposition(qfs::write::WriteDisposition::CreateNew),
            )))
        })
    }

    /// Returns the concrete asynchronous filesystem facade under test.
    ///
    /// # Returns
    ///
    /// The isolated asynchronous facade owned by this fixture.
    fn file_system(&self) -> &AsyncFileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    ///
    /// # Parameters
    ///
    /// * `relative` - Suite-generated name relative to the fixture namespace.
    ///
    /// # Returns
    ///
    /// The corresponding logical path within the isolated namespace.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the name cannot be
    /// represented by the provider's path model.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Reports whether copy is intentionally limited to the stream fallback.
    ///
    /// # Returns
    ///
    /// `true` when copy contract checks must use the stream fallback.
    #[inline]
    #[must_use]
    fn copy_fallback_only(&self) -> bool {
        false
    }

    /// Maps a relative list prefix for the supplied root.
    ///
    /// # Parameters
    ///
    /// * `root` - Logical directory passed to the list operation.
    /// * `relative` - Testkit-relative descendant selected by the contract.
    ///
    /// # Returns
    ///
    /// The provider-specific prefix expected by its list implementation.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the prefix cannot be
    /// represented for the supplied root.
    #[inline]
    fn list_prefix(&self, root: &Path, relative: &str) -> FixtureResult<String> {
        let _ = root;
        Ok(relative.to_owned())
    }

    /// Asynchronously seeds a complete file outside the operation under test.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path for the seeded file.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// A future resolving to `Supported(path)` when seeding succeeds, or
    /// `Unsupported` when the fixture cannot prepare files out of band.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when
    /// provider-specific setup fails.
    #[inline]
    fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = (relative, bytes);
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously observes a complete file outside the operation under
    /// test.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical path to observe.
    ///
    /// # Returns
    ///
    /// A future resolving to `Supported(bytes)` with complete content, or
    /// `Unsupported` when the fixture cannot observe files out of band.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when
    /// provider-specific observation fails.
    #[inline]
    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously observes whether a resource exists out of band.
    ///
    /// # Parameters
    ///
    /// * `path` - Resource to observe independently of the facade under test.
    ///
    /// # Returns
    ///
    /// A future resolving to `Supported(true/false)` when observable, or
    /// `Unsupported` when independent observation is unavailable.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when
    /// observation fails.
    #[inline]
    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously writes a complete resource through fixture setup.
    ///
    /// # Parameters
    ///
    /// * `path` - Resource to seed independently of the facade under test.
    /// * `bytes` - Complete content to write.
    ///
    /// # Returns
    ///
    /// A future resolving to `Supported(())` when setup succeeds, or
    /// `Unsupported` when out-of-band writing is unavailable.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when setup
    /// fails.
    #[inline]
    fn write_file_out_of_band<'a>(&'a self, path: &'a Path, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<()>> {
        let _ = (path, bytes);
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously observes the current provider resource version.
    ///
    /// # Parameters
    ///
    /// * `path` - Resource whose provider version is observed.
    ///
    /// # Returns
    ///
    /// A future resolving to the current version, or `Unsupported` when the
    /// fixture cannot independently obtain it.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when version
    /// observation fails.
    #[inline]
    fn resource_version<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously returns a valid version that cannot match the resource.
    ///
    /// # Parameters
    ///
    /// * `path` - Resource whose current version the supplied token must not
    ///   match.
    ///
    /// # Returns
    ///
    /// A future resolving to a valid stale version, or `Unsupported` when the
    /// fixture cannot produce one.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when producing
    /// or validating the version fails.
    #[inline]
    fn stale_resource_version<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously supplies a resource whose checksum is invalid.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative name for the corrupted resource.
    ///
    /// # Returns
    ///
    /// A future resolving to the prepared path, or `Unsupported` when the
    /// fixture cannot create a checksum-corrupt resource.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when
    /// preparation fails.
    #[inline]
    fn checksum_failure_case<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = relative;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Independently releases every resource owned by this fixture.
    ///
    /// This mandatory operation must be idempotent and must reclaim partially
    /// prepared resources and staging state, including resources unknown to
    /// the suite. Use a native or out-of-band channel independent of the
    /// facade under test. Return an error if cleanup could not be confirmed.
    /// A cancelled asynchronous attempt must remain safe to retry.
    ///
    /// # Returns
    ///
    /// A future completing after fixture-owned resources and staging state
    /// have been independently reclaimed.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when cleanup
    /// cannot be confirmed.
    fn teardown(&self) -> FixtureFuture<'_, ()>;

    /// Asynchronously seeds an empty directory or prefix.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative name of the directory or prefix.
    ///
    /// # Returns
    ///
    /// A future resolving to its path, or `Unsupported` when independent
    /// seeding is unavailable.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when seeding
    /// fails.
    #[inline]
    fn seed_empty_directory<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = relative;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously seeds a symbolic link.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative name for the symbolic link.
    ///
    /// # Returns
    ///
    /// A future resolving to the link path, or `Unsupported` when the fixture
    /// cannot seed symbolic links.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when seeding
    /// fails.
    #[inline]
    fn seed_symlink<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = relative;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Supplies an asynchronously prepared native copy fast-path case.
    ///
    /// # Parameters
    ///
    /// * `method` - Provider-native copy method the case must exercise.
    ///
    /// # Returns
    ///
    /// A future resolving to a case for that method, or `Unsupported` when
    /// the fixture cannot provide the native path.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when case
    /// preparation fails.
    #[inline]
    fn copy_fast_path_case<'a>(&'a self, method: CopyMethod) -> FixtureFuture<'a, FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously prepares a stage-aware single-file cancellation probe.
    ///
    /// A supported probe must also support independent `read_file` and
    /// `exists_out_of_band` observations for its source and destination.
    /// The source must contain a complete file and the destination must have
    /// a known initial state; otherwise return `Unsupported`.
    ///
    /// # Parameters
    ///
    /// * `stage` - Provider-owned copy stage that must be held pending.
    /// * `relative` - Testkit-relative name used to isolate probe resources.
    ///
    /// # Returns
    ///
    /// A future resolving to a probe for the requested stage, or
    /// `Unsupported` when stage control or independent observations are
    /// unavailable.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when probe
    /// setup fails; setup errors must not be reported as `Unsupported`.
    #[inline]
    fn prepare_copy_cancellation<'a>(
        &'a self,
        stage: AsyncCopyCancellationStage,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Box<dyn CopyCancellationProbe>>> {
        let _ = (stage, relative);
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Prepares a stage-aware whole-file write cancellation probe. A supported
    /// probe independently observes the target and reports bytes accepted by
    /// the provider writer. Return `Unsupported` when reliable stage
    /// acknowledgement or evidence is unavailable.
    ///
    /// # Parameters
    ///
    /// * `stage` - Provider-owned write stage that must be held pending.
    /// * `relative` - Testkit-relative target name used to isolate the probe.
    ///
    /// # Returns
    ///
    /// A future resolving to a probe for the requested stage, or
    /// `Unsupported` when reliable stage control or independent evidence is
    /// unavailable.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when probe
    /// setup fails; setup errors must not be reported as `Unsupported`.
    #[inline]
    fn prepare_write_cancellation<'a>(
        &'a self,
        stage: AsyncWriteCancellationStage,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Box<dyn WriteCancellationProbe>>> {
        let _ = (stage, relative);
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }
}
