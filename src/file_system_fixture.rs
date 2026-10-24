// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed synchronous fixtures for filesystem contract suites.

use qubit_fs as qfs;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::path::Path;

use crate::CopyFixtureCase;
use crate::FixtureResult;
use crate::FixtureSupport;

/// Supplies an isolated facade and provider-specific contract observations.
///
/// # Examples
///
/// ```
/// # mod support { include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/rustdoc_support.rs")); }
/// # use support::RustdocSyncFixture;
/// use qubit_fs_testkit::{FileSystemContractSuite, FileSystemFixture};
///
/// let fixture = RustdocSyncFixture::new();
/// let suite = FileSystemContractSuite::new(&fixture);
/// assert!(suite.run().report().checks().is_empty());
/// ```
pub trait FileSystemFixture {
    /// Snapshots every logical path in the isolated configured namespace.
    ///
    /// Use the underlying SDK or fixture model, never the facade list method.
    /// Include resources created by earlier checks, not only the current seed.
    /// Returns unsupported when independent completeness cannot be established.
    fn snapshot_namespace_paths(&self) -> FixtureResult<FixtureSupport<Vec<Path>>> {
        Ok(FixtureSupport::Unsupported)
    }

    /// Independently prepares a copy source and a scenario-specific target.
    ///
    /// File scenarios contain the supplied bytes and use file options, with
    /// required atomicity or durability for the respective strong scenario.
    /// `ServerSide` cases use provider-selected observable bytes and require
    /// server-side copy. Ready cases have distinct paths,
    /// and Conflict targets contain `b"existing"`.
    /// Teardown must clean partial setup even when preparation fails.
    ///
    /// # Parameters
    ///
    /// * `scenario` - Copy behavior the contract suite will exercise.
    /// * `source_relative` - Testkit-relative source path to prepare.
    /// * `target_relative` - Testkit-relative destination path to prepare.
    /// * `bytes` - File content used to seed file copy scenarios.
    ///
    /// # Returns
    ///
    /// A ready copy case or an unavailable result when the fixture cannot
    /// independently prepare the requested scenario.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when fixture setup fails.
    fn prepare_copy(
        &self,
        scenario: crate::CopyScenario,
        source_relative: &str,
        target_relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<crate::FixturePreparation<crate::CopyFixtureCase>> {
        if scenario == crate::CopyScenario::ServerSide {
            return Ok(match self.copy_fast_path_case(qfs::copy::CopyMethod::ServerSide)? {
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
            let FixtureSupport::Supported(source) = self.seed_empty_directory(source_relative)? else {
                return Ok(crate::FixturePreparation::Unavailable {
                    reason: "tree root setup unavailable".to_owned(),
                });
            };
            let sub_relative = format!("{source_relative}/sub");
            if matches!(self.seed_empty_directory(&sub_relative)?, FixtureSupport::Unsupported) {
                return Ok(crate::FixturePreparation::Unavailable {
                    reason: "tree subdirectory setup unavailable".to_owned(),
                });
            }
            let child_relative = format!("{source_relative}/sub/child");
            if matches!(self.seed_file(&child_relative, bytes)?, FixtureSupport::Unsupported) {
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
        let FixtureSupport::Supported(source) = self.seed_file(source_relative, bytes)? else {
            return Ok(crate::FixturePreparation::Unavailable {
                reason: "copy source seed unavailable".to_owned(),
            });
        };
        let target = if scenario == crate::CopyScenario::Conflict {
            let FixtureSupport::Supported(target) = self.seed_file(target_relative, b"existing")? else {
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
    }

    /// Independently prepares an existing file for a selected deletion
    /// scenario. Version hooks must provide evidence for
    /// `DeleteScenario::IfMatch`.
    ///
    /// # Parameters
    ///
    /// * `_scenario` - Deletion behavior the contract suite will exercise.
    /// * `relative` - Testkit-relative path for the file to seed.
    /// * `bytes` - Exact initial content of the file.
    ///
    /// # Returns
    ///
    /// A ready file path or an unavailable result when independent seeding is
    /// unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when fixture setup fails.
    fn prepare_delete(
        &self,
        _scenario: crate::DeleteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<crate::FixturePreparation<Path>> {
        Ok(match self.seed_file(relative, bytes)? {
            FixtureSupport::Supported(path) => crate::FixturePreparation::Ready(path),
            FixtureSupport::Unsupported => crate::FixturePreparation::Unavailable {
                reason: "fixture cannot seed deletion scenario".to_owned(),
            },
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
    /// * `scenario` - Read behavior for which the resource is prepared.
    /// * `relative` - Testkit-relative path for the resource.
    /// * `bytes` - Expected initial content except in the corruption scenario.
    ///
    /// # Returns
    ///
    /// A ready resource path or an unavailable result when independent setup
    /// is unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when fixture setup fails.
    fn prepare_read(
        &self,
        scenario: crate::ReadScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<crate::FixturePreparation<Path>> {
        let prepared = if scenario == crate::ReadScenario::ChecksumCorruption {
            self.checksum_failure_case(relative)?
        } else {
            self.seed_file(relative, bytes)?
        };
        Ok(match prepared {
            FixtureSupport::Supported(path) => crate::FixturePreparation::Ready(path),
            FixtureSupport::Unsupported => crate::FixturePreparation::Unavailable {
                reason: "fixture cannot independently prepare this read scenario".to_owned(),
            },
        })
    }

    /// Prepares one write scenario without invoking the tested write facade.
    ///
    /// The default prepares CreateNew and durable creation requests. Atomic
    /// replacement, creation conflicts and append use independent seeding.
    /// Other scenarios require provider-specific setup and evidence.
    ///
    /// # Parameters
    ///
    /// * `scenario` - Write behavior the contract suite will exercise.
    /// * `relative` - Testkit-relative path for the resource.
    /// * `bytes` - Content the write case supplies to the tested operation.
    ///
    /// # Returns
    ///
    /// A ready write case or an unavailable result when the fixture cannot
    /// independently prepare the requested scenario.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when fixture setup fails.
    fn prepare_write(
        &self,
        scenario: crate::WriteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<crate::FixturePreparation<crate::WriteFixtureCase>> {
        if scenario == crate::WriteScenario::Replace {
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"previous contents")? else {
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
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"a")? else {
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
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"before")? else {
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
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"a")? else {
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
    }

    /// Returns the concrete synchronous filesystem facade under test.
    ///
    /// # Returns
    ///
    /// The isolated filesystem facade owned by this fixture.
    fn file_system(&self) -> &FileSystem;

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

    /// Reports whether copy requests must use the stream fallback.
    ///
    /// Override this when the fixture is designed to exercise only fallback
    /// copying, so contract checks do not expect provider-native fast paths.
    ///
    /// # Returns
    ///
    /// `true` when the suite must restrict copy checks to stream fallback;
    /// otherwise, `false`.
    #[must_use]
    #[inline]
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

    /// Seeds a complete file outside the operation currently under test.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path for the seeded file.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// `Supported(path)` when seeding succeeds, or `Unsupported` when the
    /// fixture cannot prepare files out of band.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// setup fails.
    #[inline]
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        let _ = (relative, bytes);
        Ok(FixtureSupport::Unsupported)
    }

    /// Reads a complete file outside the operation currently under test.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical path to observe.
    ///
    /// # Returns
    ///
    /// `Supported(bytes)` with the complete content, or `Unsupported` when the
    /// fixture cannot observe files out of band.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// observation fails.
    #[inline]
    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Observes whether a resource exists without using the tested facade.
    ///
    /// This independent observation lets a contract check distinguish the
    /// operation's effect from the facade's own reported result.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical resource path to inspect.
    ///
    /// # Returns
    ///
    /// `Supported(true)` or `Supported(false)` when existence can be observed,
    /// or `Unsupported` when no independent observation is available.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the provider's
    /// independent observation fails.
    #[inline]
    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Writes a complete resource through fixture-owned setup facilities.
    ///
    /// This hook prepares or restores state without exercising the facade
    /// operation whose contract is under test.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical resource path to write.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// `Supported(())` when the out-of-band write is available, or
    /// `Unsupported` when the fixture cannot write independently.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when fixture-owned setup
    /// fails.
    #[inline]
    fn write_file_out_of_band(&self, path: &Path, bytes: &[u8]) -> FixtureResult<FixtureSupport<()>> {
        let _ = (path, bytes);
        Ok(FixtureSupport::Unsupported)
    }

    /// Observes the current provider version outside the operation under test.
    ///
    /// The returned token is evidence for version-conditional contract checks
    /// and must come from an observation independent of the tested operation.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical resource path whose version is observed.
    ///
    /// # Returns
    ///
    /// `Supported(version)` when a provider version is available, or
    /// `Unsupported` when it cannot be observed independently.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when version observation
    /// fails.
    #[inline]
    fn resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Returns a valid version that does not match the current resource.
    ///
    /// The token must be valid for the provider's conditional operation while
    /// being guaranteed not to identify the resource's current version.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical resource path whose current version must not match.
    ///
    /// # Returns
    ///
    /// `Supported(version)` with a valid stale token, or `Unsupported` when
    /// the fixture cannot establish that condition.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when stale-token
    /// preparation fails.
    #[inline]
    fn stale_resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies an independently prepared resource whose checksum is invalid.
    ///
    /// The resource must be readable as a checksum-corruption scenario without
    /// using the read operation under test to create the corruption.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path for the corrupted resource.
    ///
    /// # Returns
    ///
    /// `Supported(path)` when an invalid-checksum resource is prepared, or
    /// `Unsupported` when the provider cannot offer this scenario.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when preparation fails.
    #[inline]
    fn checksum_failure_case(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let _ = relative;
        Ok(FixtureSupport::Unsupported)
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
    /// `Ok(())` when cleanup is confirmed; otherwise, a fixture error
    /// describing the cleanup failure.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when any owned resource
    /// or staging state could not be reclaimed or confirmed.
    fn teardown(&self) -> FixtureResult<()>;

    /// Seeds an empty directory or prefix outside the operation under test.
    ///
    /// Providers may represent directories as explicit resources or prefixes;
    /// the returned path identifies the seeded logical root in either model.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path for the empty directory.
    ///
    /// # Returns
    ///
    /// `Supported(path)` when setup succeeds, or `Unsupported` when this
    /// provider cannot seed an empty directory.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when setup fails.
    #[inline]
    fn seed_empty_directory(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let _ = relative;
        Ok(FixtureSupport::Unsupported)
    }

    /// Seeds a symbolic link outside the operation under test.
    ///
    /// The link target is chosen by the provider; this hook reports support
    /// only when the resulting link can be used by the relevant contract.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path at which to create the link.
    ///
    /// # Returns
    ///
    /// `Supported(path)` when the link is prepared, or `Unsupported` when the
    /// provider cannot create a suitable symbolic link.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when link setup fails.
    #[inline]
    fn seed_symlink(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let _ = relative;
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a case in which the requested native copy method must apply.
    ///
    /// # Parameters
    ///
    /// * `method` - Native copy method the prepared request must exercise.
    ///
    /// # Returns
    ///
    /// `Supported(case)` when the fixture can prepare an applicable request,
    /// or `Unsupported` when no such provider-specific case is available.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// case preparation fails.
    #[inline]
    fn copy_fast_path_case(&self, method: CopyMethod) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Ok(FixtureSupport::Unsupported)
    }
}
