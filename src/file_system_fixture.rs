// qubit-style: allow explicit-imports
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
    /// Required atomicity or durability for the respective strong scenario.
    /// ServerSide cases use provider-selected observable bytes and require
    /// server-side copy. Ready cases have distinct paths,
    /// and Conflict targets contain `b"existing"`.
    /// Teardown must clean partial setup even when preparation fails.
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

    /// Reports whether copy is intentionally limited to the stream fallback.
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
    #[inline]
    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Writes a complete resource through fixture-owned setup facilities.
    #[inline]
    fn write_file_out_of_band(&self, path: &Path, bytes: &[u8]) -> FixtureResult<FixtureSupport<()>> {
        let _ = (path, bytes);
        Ok(FixtureSupport::Unsupported)
    }

    /// Observes the current provider version outside the operation under test.
    #[inline]
    fn resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Returns a valid version that does not match the current resource.
    #[inline]
    fn stale_resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies an independently prepared resource whose checksum is invalid.
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
    fn teardown(&self) -> FixtureResult<()>;

    /// Seeds an empty directory or prefix outside the operation under test.
    #[inline]
    fn seed_empty_directory(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let _ = relative;
        Ok(FixtureSupport::Unsupported)
    }

    /// Seeds a symbolic link outside the operation under test.
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
